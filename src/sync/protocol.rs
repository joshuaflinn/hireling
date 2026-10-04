//! Wire protocol frames for the party sync socket — E7, extended by E8
//! (gh#10): effect write frames (`effect_new` create / `effect` update+end)
//! and the `derived` frame per wire-protocol.md §8's named extension point.
//! The types here are the contract: `specs/007-party-sync/contracts/wire-protocol.md`
//! §2's shapes, verbatim (tag `"t"`, `snake_case` fields). Decode is deny-by-default:
//! unknown frame types are errors before any handler runs.

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
    /// Effect CREATE (E8): the effect does not exist yet, so the target is
    /// the party it lands in; the new row's id is minted on apply and all
    /// later addressing uses [`FieldTarget::Effect`].
    EffectNew {
        party_id: i64,
    },
}

/// The versioned vitals columns; money is one four-denomination unit (E2's
/// schema: `money_*_version` is a single column). `focus_current`,
/// `hero_points`, and `daily` are E6's Q1 spell-economy fields (design §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VitalsField {
    Hp,
    TempHp,
    Money,
    LevelAdjust,
    FocusCurrent,
    HeroPoints,
    Daily,
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
            Self::FocusCurrent => "focus_current",
            Self::HeroPoints => "hero_points",
            Self::Daily => "daily",
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
    /// The client's answer to the server's liveness ping (design.md:
    /// app-level ping/pong JSON frames both directions).
    Pong,
}

impl ClientFrame {
    /// Decode one JSON text frame. Deny-by-default: malformed JSON, unknown
    /// `"t"` tags, and incomplete writes are errors here — nothing reaches a
    /// handler unvalidated. Effect writes decode since E8 (wire-protocol.md
    /// §8's extension realized); their per-op validation lives in the write
    /// path's bounds table, which owns the `rejected` reasons.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError::Malformed`] for anything that is not a
    /// valid client frame.
    pub fn decode(raw: &str) -> Result<Self, ProtocolError> {
        serde_json::from_str(raw).map_err(ProtocolError::Malformed)
    }
}

/// Why a client frame was refused at decode.
#[derive(Debug)]
pub enum ProtocolError {
    /// Not valid JSON, an unknown `"t"`, or a shape no frame type defines.
    Malformed(serde_json::Error),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(err) => write!(f, "malformed frame: {err}"),
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Malformed(err) => Some(err),
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
        /// Every roster character's engine output (E8): a reconnecting
        /// client gets the full derived picture and loses nothing by
        /// skipping the streamed `derived` frames it missed.
        derived: Vec<Box<hireling_engine::model::EngineOutput>>,
    },
    Diff {
        field: FieldTarget,
        value: JsonValue,
        version: i64,
        actor_sub: String,
        op_id: Option<String>,
    },
    Ack(Ack),
    /// One character's recomputed engine output (E8): the derived numbers
    /// the sheet renders with their full math, plus the effect chips. A pure
    /// function of already-versioned state — fans out after the effect
    /// `diff` that caused it (TCP-ordered per connection).
    Derived {
        character_id: i64,
        /// Boxed: the derived output is the frame's bulk — keeps the enum
        /// cheap to move through the registry queues (the
        /// `large_enum_variant` lint), wire shape unchanged.
        output: Box<hireling_engine::model::EngineOutput>,
    },
    /// The server's liveness probe (contract §5, binding 20 s / 10 s);
    /// the client answers with `pong` (any-frame watchdog on its side).
    Ping,
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
            ClientFrame::Ping | ClientFrame::Pong => {
                panic!("expected a write frame, got a liveness frame")
            }
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
    fn the_liveness_frames_round_trip_both_ways() {
        // Client answers the server's liveness probe.
        let pong: ClientFrame = serde_json::from_value(json!({"t": "pong"})).expect("pong decodes");
        assert!(matches!(pong, ClientFrame::Pong));
        assert_eq!(
            serde_json::to_value(&pong).expect("encodes"),
            json!({"t": "pong"})
        );
        // The server's liveness probe.
        assert_eq!(
            serde_json::to_value(&ServerFrame::Ping).expect("encodes"),
            json!({"t": "ping"})
        );
    }

    #[test]
    fn an_effect_write_decodes_and_effect_new_round_trips() {
        // E8 realized the §8 extension point: effect writes decode (their
        // per-op validation lives in the write path's bounds table, not
        // here) — update+end address the row, create addresses the party.
        let update = json!({
            "t": "write",
            "op_id": "0b9e6b1e-1c1d-4c1e-9f6e-000000000004",
            "target": {"kind": "effect", "effect_id": 41},
            "base_version": 1042,
            "value": {"op": "update", "targets": [7, 9]}
        });
        let frame =
            ClientFrame::decode(&update.to_string()).expect("effect writes decode since E8");
        assert!(matches!(
            frame,
            ClientFrame::Write {
                target: FieldTarget::Effect { effect_id: 41 },
                ..
            }
        ));

        let create = json!({
            "t": "write",
            "op_id": "0b9e6b1e-1c1d-4c1e-9f6e-000000000005",
            "target": {"kind": "effect_new", "party_id": 1},
            "base_version": 0,
            "value": {"op": "create", "name": "Bless", "source_character_id": 3,
                      "targets": [7, 9],
                      "modifiers": [{"type": "status", "stat": "attack", "value": 1}],
                      "duration_note": "10 rounds", "corpus_entry_id": null,
                      "condition_value": null}
        });
        let create_frame = ClientFrame::decode(&create.to_string()).expect("create decodes");
        assert!(matches!(
            create_frame,
            ClientFrame::Write {
                target: FieldTarget::EffectNew { party_id: 1 },
                ..
            }
        ));
        // The full contract literal round-trips byte-shape stable.
        assert_eq!(
            serde_json::to_value(&create_frame).expect("encodes"),
            create
        );
    }

    #[test]
    fn the_derived_frame_round_trips() {
        let frame = ServerFrame::Derived {
            character_id: 7,
            output: Box::new(hireling_engine::model::EngineOutput {
                schema: hireling_engine::model::OUTPUT_SCHEMA.to_owned(),
                character_id: 7,
                derived: serde_json::from_value(json!({
                    "ac": {"base": 10, "total": 11,
                            "applied": [{"type": "status", "value": 1, "effect_id": 41,
                                         "effect_name": "Bless", "source_character_id": 3}],
                            "suppressed": []},
                    "fort": {"base": 9, "total": 9, "applied": [], "suppressed": []},
                    "ref": {"base": 8, "total": 8, "applied": [], "suppressed": []},
                    "will": {"base": 11, "total": 11, "applied": [], "suppressed": []},
                    "perception": {"base": 11, "total": 11, "applied": [], "suppressed": []},
                    "speed": {"base": 25, "total": 25, "applied": [], "suppressed": []},
                    "class_dc": {"base": null, "total": null, "applied": [], "suppressed": []},
                    "strikes": [], "casters": [], "skills": []
                }))
                .expect("minimal derived shape"),
                effects: vec![],
                render_base: hireling_engine::model::RenderBase {
                    level: 3,
                    hp_max: 32,
                    focus_max: 2,
                    hero_max: 3,
                    cantrip_rank: 2,
                    attributes: hireling_engine::model::Attributes::default(),
                },
            }),
        };
        let encoded = serde_json::to_value(&frame).expect("encodes");
        assert_eq!(
            encoded,
            json!({
                "t": "derived",
                "character_id": 7,
                "output": {
                    "schema": "hireling.engine.output.v1",
                    "character_id": 7,
                    "derived": {
                        "ac": {"base": 10, "total": 11,
                                "applied": [{"type": "status", "value": 1, "effect_id": 41,
                                             "effect_name": "Bless", "source_character_id": 3}],
                                "suppressed": []},
                        "fort": {"base": 9, "total": 9, "applied": [], "suppressed": []},
                        "ref": {"base": 8, "total": 8, "applied": [], "suppressed": []},
                        "will": {"base": 11, "total": 11, "applied": [], "suppressed": []},
                        "perception": {"base": 11, "total": 11, "applied": [], "suppressed": []},
                        "speed": {"base": 25, "total": 25, "applied": [], "suppressed": []},
                        "class_dc": {"base": null, "total": null, "applied": [], "suppressed": []},
                        "strikes": [], "casters": [], "skills": []
                    },
                    "effects": [],
                    "render_base": {
                        "level": 3, "hp_max": 32, "focus_max": 2, "hero_max": 3,
                        "cantrip_rank": 2,
                        "attributes": {"str": 0, "dex": 0, "con": 0, "int": 0, "wis": 0, "cha": 0}
                    }
                }
            }),
            "the derived frame is the EngineOutput contract shape, verbatim"
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
            derived: vec![],
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
