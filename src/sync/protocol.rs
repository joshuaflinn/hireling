//! Wire protocol frames for the party sync socket — E7.
//!
//! The types here are the contract: `specs/007-party-sync/contracts/wire-protocol.md`
//! §2's shapes, verbatim (tag `"t"`, `snake_case` fields). Decode is deny-by-default:
//! unknown frame types and effect writes are errors before any handler runs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Which versioned live-state field a frame addresses. The wire shape is
/// internally tagged by `"kind"` (contract §3). `character_id` rides every
/// variant: a party snapshot spans every member's fields, so a target must
/// name its row unambiguously — and §4's "unknown target under the writer's
/// ownership" implies per-character addressing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldTarget {
    Vitals {
        character_id: i64,
        field: VitalsField,
    },
    Slot {
        character_id: i64,
        caster_key: String,
        rank: i32,
        slot_index: i32,
    },
    Inv {
        character_id: i64,
        item_name: String,
    },
    Effect {
        effect_id: i64,
    },
}

/// The versioned vitals columns; money is one four-denomination unit (E2's
/// schema: `money_*_version` is a single column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VitalsField {
    Hp,
    TempHp,
    Money,
    LevelAdjust,
}

impl VitalsField {
    /// The wire/ledger name of this field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hp => "hp",
            Self::TempHp => "temp_hp",
            Self::Money => "money",
            Self::LevelAdjust => "level_adjust",
        }
    }
}

/// One row of a snapshot: a field target, its current value, its current
/// version — the diff shape minus attribution (contract §2, `fields`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotField {
    pub field: FieldTarget,
    pub value: JsonValue,
    pub version: i64,
}

/// The account a `hello` greets (contract §2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorInfo {
    pub sub: String,
    pub role: String,
}

/// A frame the client sends (contract §2, client → server).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientFrame {
    Write {
        op_id: String,
        target: FieldTarget,
        base_version: i64,
        value: JsonValue,
    },
    Ping,
}

impl ClientFrame {
    /// Decode one JSON text frame. Deny-by-default: malformed JSON, unknown
    /// `"t"` tags, incomplete writes, and effect writes (E8's extension
    /// point, named distinctly) are all errors here — nothing reaches a
    /// handler unvalidated.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::EffectWritesDeferred`] for `kind:"effect"`
    /// writes; [`ProtocolError::Malformed`] for anything else that is not a
    /// valid client frame.
    pub fn decode(raw: &str) -> Result<Self, ProtocolError> {
        let value: JsonValue = serde_json::from_str(raw).map_err(ProtocolError::Malformed)?;
        let is_effect_write = value.get("t").and_then(JsonValue::as_str) == Some("write")
            && value
                .get("target")
                .and_then(|target| target.get("kind"))
                .and_then(JsonValue::as_str)
                == Some("effect");
        if is_effect_write {
            return Err(ProtocolError::EffectWritesDeferred);
        }
        serde_json::from_value(value).map_err(ProtocolError::Malformed)
    }
}

/// Why a client frame was refused at decode.
#[derive(Debug)]
pub enum ProtocolError {
    /// Not valid JSON, an unknown `"t"`, or a shape no frame type defines.
    Malformed(serde_json::Error),
    /// A `kind:"effect"` write: the protocol carries effects read-only in
    /// E7; write semantics are E8's (contract §8) — denied with its own
    /// reason, never silently mangled into "malformed".
    EffectWritesDeferred,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(err) => write!(f, "malformed frame: {err}"),
            Self::EffectWritesDeferred => {
                write!(f, "effect writes are deferred to E8")
            }
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Malformed(err) => Some(err),
            Self::EffectWritesDeferred => None,
        }
    }
}

/// The terminal disposition of a client operation (contract §2/§4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Applied,
    Superseded,
    AlreadyApplied,
    Rejected,
    Forbidden,
}

impl Outcome {
    /// The ledger's stored text for this outcome (`client_ops.outcome`).
    /// `already_applied` is a read answer — no caller stores it, but the
    /// mapping is total.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Superseded => "superseded",
            Self::AlreadyApplied => "already_applied",
            Self::Rejected => "rejected",
            Self::Forbidden => "forbidden",
        }
    }
}

/// A frame the server sends (contract §2, server → client).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerFrame {
    Hello {
        party_id: i64,
        you: ActorInfo,
        server_now: DateTime<Utc>,
    },
    Snapshot {
        fields: Vec<SnapshotField>,
        snapshot_bytes: u64,
    },
    Diff {
        field: FieldTarget,
        value: JsonValue,
        version: i64,
        actor_sub: String,
        op_id: Option<String>,
    },
    Ack(Ack),
    Pong,
    Bye {
        reason: String,
    },
}

/// The writer's authoritative answer for one operation. Flattened on the
/// wire into the `ack` frame (contract §2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ack {
    pub op_id: String,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winning_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Client → server: every frame kind round-trips to the contract's
    // -- literal §2 JSON (fixtures copied from the contract).

    #[test]
    fn a_vitals_write_round_trips_to_the_contract_shape() {
        let raw = json!({
            "t": "write",
            "op_id": "0b9e6b1e-1c1d-4c1e-9f6e-000000000001",
            "target": {"kind": "vitals", "character_id": 3, "field": "hp"},
            "base_version": 1042,
            "value": 14
        });
        let frame: ClientFrame = serde_json::from_value(raw.clone()).expect("vitals write decodes");
        match &frame {
            ClientFrame::Write {
                op_id,
                target,
                base_version,
                value,
            } => {
                assert_eq!(op_id, "0b9e6b1e-1c1d-4c1e-9f6e-000000000001");
                assert_eq!(
                    target,
                    &FieldTarget::Vitals {
                        character_id: 3,
                        field: VitalsField::Hp
                    }
                );
                assert_eq!(*base_version, 1042);
                assert_eq!(value, &json!(14));
            }
            ClientFrame::Ping => panic!("expected a write frame, got ping"),
        }
        assert_eq!(serde_json::to_value(&frame).expect("encodes"), raw);
    }

    #[test]
    fn a_slot_write_round_trips_to_the_contract_shape() {
        let raw = json!({
            "t": "write",
            "op_id": "0b9e6b1e-1c1d-4c1e-9f6e-000000000002",
            "target": {"kind": "slot", "character_id": 3, "caster_key": "Wizard",
                       "rank": 3, "slot_index": 0},
            "base_version": 871,
            "value": {"used": true}
        });
        let frame: ClientFrame = serde_json::from_value(raw.clone()).expect("slot write decodes");
        assert!(matches!(
            frame,
            ClientFrame::Write {
                target: FieldTarget::Slot {
                    rank: 3,
                    slot_index: 0,
                    ..
                },
                ..
            }
        ));
        assert_eq!(serde_json::to_value(&frame).expect("encodes"), raw);
    }

    #[test]
    fn an_inv_write_round_trips_to_the_contract_shape() {
        let raw = json!({
            "t": "write",
            "op_id": "0b9e6b1e-1c1d-4c1e-9f6e-000000000003",
            "target": {"kind": "inv", "character_id": 3, "item_name": "Chalk"},
            "base_version": 990,
            "value": {"qty_delta": -2}
        });
        let frame: ClientFrame = serde_json::from_value(raw.clone()).expect("inv write decodes");
        assert!(matches!(
            frame,
            ClientFrame::Write {
                target: FieldTarget::Inv { .. },
                ..
            }
        ));
        assert_eq!(serde_json::to_value(&frame).expect("encodes"), raw);
    }

    #[test]
    fn a_ping_round_trips_and_unknown_tags_are_denied() {
        let ping: ClientFrame = serde_json::from_value(json!({"t": "ping"})).expect("ping decodes");
        assert!(matches!(ping, ClientFrame::Ping));
        assert_eq!(
            serde_json::to_value(&ping).expect("encodes"),
            json!({"t": "ping"})
        );

        let unknown = serde_json::from_value::<ClientFrame>(json!({"t": "wat"}));
        assert!(unknown.is_err(), "unknown frame type must be denied");
    }

    #[test]
    fn an_effect_write_is_denied_with_the_deferred_reason() {
        let raw = json!({
            "t": "write",
            "op_id": "0b9e6b1e-1c1d-4c1e-9f6e-000000000004",
            "target": {"kind": "effect", "effect_id": 7},
            "base_version": 44,
            "value": {"active": false}
        });
        let err = ClientFrame::decode(&raw.to_string())
            .expect_err("effect writes are E8's extension point");
        assert!(
            matches!(err, ProtocolError::EffectWritesDeferred),
            "the denial must name the deferral, got: {err}"
        );
    }

    #[test]
    fn malformed_and_incomplete_writes_are_denied() {
        assert!(ClientFrame::decode("not json at all").is_err());
        // Missing base_version: a write without its CAS base is unusable.
        let missing_base = json!({
            "t": "write",
            "op_id": "0b9e6b1e-1c1d-4c1e-9f6e-000000000005",
            "target": {"kind": "vitals", "character_id": 3, "field": "hp"},
            "value": 14
        });
        assert!(ClientFrame::decode(&missing_base.to_string()).is_err());
    }

    // -- Server → client: every frame kind round-trips to the contract's
    // -- literal §2 JSON.

    #[test]
    fn a_hello_round_trips_to_the_contract_shape() {
        let frame = ServerFrame::Hello {
            party_id: 1,
            you: ActorInfo {
                sub: "dev-sub-josh".to_owned(),
                role: "player".to_owned(),
            },
            server_now: chrono::DateTime::parse_from_rfc3339("2026-10-01T19:04:05Z")
                .expect("fixed test instant")
                .with_timezone(&chrono::Utc),
        };
        let encoded = serde_json::to_value(&frame).expect("encodes");
        assert_eq!(
            encoded,
            json!({
                "t": "hello",
                "party_id": 1,
                "you": {"sub": "dev-sub-josh", "role": "player"},
                "server_now": "2026-10-01T19:04:05Z"
            })
        );
    }

    #[test]
    fn a_snapshot_round_trips_with_field_and_effect_rows() {
        let frame = ServerFrame::Snapshot {
            fields: vec![
                SnapshotField {
                    field: FieldTarget::Vitals {
                        character_id: 3,
                        field: VitalsField::Hp,
                    },
                    value: json!(14),
                    version: 1043,
                },
                SnapshotField {
                    field: FieldTarget::Effect { effect_id: 7 },
                    value: json!({
                        "name": "Bless",
                        "source_character_id": 3,
                        "targets": [3],
                        "modifiers": [{"type": "status", "stat": "attack", "value": 1}],
                        "duration_note": "10 rounds",
                        "active": true
                    }),
                    version: 44,
                },
            ],
            snapshot_bytes: 18_4223 % 100_000,
        };
        let encoded = serde_json::to_value(&frame).expect("encodes");
        let fields = encoded
            .get("fields")
            .and_then(|fields| fields.as_array())
            .expect("fields array");
        assert_eq!(fields.len(), 2, "one vitals row + one effect row");
        assert_eq!(
            fields.first(),
            Some(&json!({
                "field": {"kind": "vitals", "character_id": 3, "field": "hp"},
                "value": 14,
                "version": 1043
            }))
        );
        assert_eq!(encoded.get("snapshot_bytes"), Some(&json!(84_223)));
    }

    #[test]
    fn a_diff_and_the_five_acks_round_trip() {
        let diff = ServerFrame::Diff {
            field: FieldTarget::Vitals {
                character_id: 3,
                field: VitalsField::Hp,
            },
            value: json!(14),
            version: 1043,
            actor_sub: "dev-sub-josh".to_owned(),
            op_id: Some("0b9e6b1e-1c1d-4c1e-9f6e-000000000001".to_owned()),
        };
        let encoded = serde_json::to_value(&diff).expect("encodes");
        assert_eq!(
            encoded,
            json!({
                "t": "diff",
                "field": {"kind": "vitals", "character_id": 3, "field": "hp"},
                "value": 14,
                "version": 1043,
                "actor_sub": "dev-sub-josh",
                "op_id": "0b9e6b1e-1c1d-4c1e-9f6e-000000000001"
            })
        );

        let cases = [
            (
                Outcome::Applied,
                json!({"t": "ack", "op_id": "op-1", "outcome": "applied", "version": 1043}),
                Ack {
                    op_id: "op-1".to_owned(),
                    outcome: Outcome::Applied,
                    version: Some(1043),
                    winning_version: None,
                    reason: None,
                },
            ),
            (
                Outcome::Superseded,
                json!({"t": "ack", "op_id": "op-1", "outcome": "superseded",
                       "winning_version": 1045}),
                Ack {
                    op_id: "op-1".to_owned(),
                    outcome: Outcome::Superseded,
                    version: None,
                    winning_version: Some(1045),
                    reason: None,
                },
            ),
            (
                Outcome::AlreadyApplied,
                json!({"t": "ack", "op_id": "op-1", "outcome": "already_applied",
                       "version": 1043}),
                Ack {
                    op_id: "op-1".to_owned(),
                    outcome: Outcome::AlreadyApplied,
                    version: Some(1043),
                    winning_version: None,
                    reason: None,
                },
            ),
            (
                Outcome::Rejected,
                json!({"t": "ack", "op_id": "op-1", "outcome": "rejected",
                       "reason": "hp must be ≥ 0"}),
                Ack {
                    op_id: "op-1".to_owned(),
                    outcome: Outcome::Rejected,
                    version: None,
                    winning_version: None,
                    reason: Some("hp must be ≥ 0".to_owned()),
                },
            ),
            (
                Outcome::Forbidden,
                json!({"t": "ack", "op_id": "op-1", "outcome": "forbidden",
                       "reason": "gm is read-only"}),
                Ack {
                    op_id: "op-1".to_owned(),
                    outcome: Outcome::Forbidden,
                    version: None,
                    winning_version: None,
                    reason: Some("gm is read-only".to_owned()),
                },
            ),
        ];
        for (outcome, wire, ack) in cases {
            assert_eq!(
                serde_json::to_value(ServerFrame::Ack(ack)).expect("encodes"),
                wire,
                "ack for {outcome:?} must match the contract literal"
            );
        }
    }

    #[test]
    fn pong_and_bye_round_trip() {
        assert_eq!(
            serde_json::to_value(&ServerFrame::Pong).expect("encodes"),
            json!({"t": "pong"})
        );
        let bye = serde_json::to_value(&ServerFrame::Bye {
            reason: "shutdown".to_owned(),
        })
        .expect("encodes");
        assert_eq!(bye, json!({"t": "bye", "reason": "shutdown"}));
    }
}
