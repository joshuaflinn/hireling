//! The wire contract gate (MOR-122): the sync socket's two sides are
//! tested in two languages, so one checked-in fixture —
//! `specs/007-party-sync/contracts/field-targets.json` — is the shared
//! source of truth. This suite pins that file to the serde wire shape of
//! [`FieldTarget`] (it asserts, never regenerates: a renamed field or a
//! new variant leaves the suite red until the fixture is updated
//! deliberately), and the vitest helper
//! `web/tests/helpers/wire-contract.js` pins every client write frame to
//! the same file. Without it, a drifted client frame dies as an
//! undecodable frame at the socket while the web suite stays green.

use serde_json::Value;
use strum::IntoEnumIterator;

use crate::sync::protocol::{FieldTarget, PartyField, VitalsField};
use crate::sync::write::validate_bounds;

/// The fixture this suite pins, relative to the crate manifest (repo root).
const FIXTURE_PATH: &str = "specs/007-party-sync/contracts/field-targets.json";

fn fixture() -> Value {
    let path = env!("CARGO_MANIFEST_DIR").to_owned() + "/" + FIXTURE_PATH;
    let raw = std::fs::read_to_string(path).expect("the contract fixture is checked in");
    serde_json::from_str(&raw).expect("the contract fixture is valid JSON")
}

/// One fixture entry: its `kind` string and exact field-name set.
fn pinned_entry<'a>(fixture: &'a Value, key: &str) -> &'a Value {
    fixture
        .pointer(&format!("/targets/{key}"))
        .unwrap_or_else(|| panic!("the fixture pins no targets entry for {key:?}"))
}

fn pinned_fields(entry: &Value, key: &str) -> Vec<String> {
    let mut fields: Vec<String> = entry
        .get("fields")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("the {key:?} fixture entry carries its field-name set"))
        .iter()
        .map(|field| field.as_str().expect("field names are strings").to_owned())
        .collect();
    fields.sort_unstable();
    fields
}

#[test]
fn every_field_target_variant_is_pinned_to_the_contract_fixture() {
    let fixture = fixture();

    // One representative per `FieldTarget` variant. The match below is
    // exhaustive over the enum: adding or renaming a variant stops this
    // file compiling until the fixture is updated deliberately — a new
    // target kind cannot slip past this gate silently.
    let examples = [
        FieldTarget::Vitals {
            character_id: 3,
            field: VitalsField::Hp,
        },
        FieldTarget::Slot {
            character_id: 3,
            caster_key: "Wizard".to_owned(),
            rank: 3,
            slot_index: 0,
        },
        FieldTarget::Inv {
            character_id: 3,
            item_name: "Chalk".to_owned(),
        },
        FieldTarget::Effect { effect_id: 41 },
        FieldTarget::EffectNew { party_id: 1 },
        FieldTarget::StashNew { party_id: 1 },
        FieldTarget::Stash {
            party_id: 1,
            item_name: "Rope".to_owned(),
        },
        FieldTarget::Bank { party_id: 1 },
        FieldTarget::Party {
            party_id: 1,
            field: PartyField::Quartermaster,
        },
        FieldTarget::Claim { claim_id: 42 },
    ];

    let mut pinned_kinds: Vec<&str> = Vec::new();
    for target in &examples {
        // Derive, never trust: the kind and the sibling field names come
        // off the serde wire shape, not from this file's intentions.
        let encoded = serde_json::to_value(target).expect("a FieldTarget encodes");
        let fields: Vec<String> = encoded
            .as_object()
            .expect("an internally tagged enum encodes as an object")
            .keys()
            .filter(|key| key.as_str() != "kind") // the tag is asserted below, not a sibling
            .cloned()
            .collect();

        let key = match target {
            FieldTarget::Vitals { .. } => "vitals",
            FieldTarget::Slot { .. } => "slot",
            FieldTarget::Inv { .. } => "inv",
            FieldTarget::Effect { .. } => "effect",
            FieldTarget::EffectNew { .. } => "effect_new",
            FieldTarget::StashNew { .. } => "stash_new",
            FieldTarget::Stash { .. } => "stash",
            FieldTarget::Bank { .. } => "bank",
            FieldTarget::Party { .. } => "party",
            FieldTarget::Claim { .. } => "claim",
        };
        pinned_kinds.push(key);

        let kind = encoded
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("the encoded target carries its kind tag: {encoded}"));
        assert_eq!(kind, key, "the serde tag must stay the pinned fixture key");

        let entry = pinned_entry(&fixture, key);
        assert_eq!(
            entry.get("kind").and_then(Value::as_str),
            Some(kind),
            "the fixture entry's kind must match the wire"
        );

        let mut wire_fields = fields;
        wire_fields.sort_unstable();
        assert_eq!(
            wire_fields,
            pinned_fields(entry, key),
            "the {kind} target's wire field set must match the fixture exactly — \
             a renamed or added serde field is a client-facing contract change"
        );
    }

    // The mirror direction: the fixture pins nothing beyond the types. An
    // entry without a variant would license a client frame no server
    // decodes.
    let mut fixture_kinds: Vec<&str> = fixture
        .get("targets")
        .and_then(Value::as_object)
        .expect("the fixture carries the targets map")
        .keys()
        .map(String::as_str)
        .collect();
    fixture_kinds.sort_unstable();
    pinned_kinds.sort_unstable();
    assert_eq!(
        fixture_kinds, pinned_kinds,
        "the fixture's targets map must name exactly the FieldTarget variants"
    );
}

#[test]
fn the_pinned_vitals_fields_are_the_serde_wire_names_of_the_enum() {
    let fixture = fixture();
    let mut pinned: Vec<String> = fixture
        .get("vitals_fields")
        .and_then(Value::as_array)
        .expect("the fixture pins the vitals field-value set")
        .iter()
        .map(|name| {
            name.as_str()
                .expect("vitals field names are strings")
                .to_owned()
        })
        .collect();
    pinned.sort_unstable();

    // Derive, never trust — twice over. The set of variants comes off
    // `EnumIter`, generated from the enum definition itself (gh#91 F1): a
    // new variant is in this set the moment it exists, with nothing in this
    // file to edit and no hand-written list that could silently omit it.
    // Each name then comes off the serde wire shape, so a
    // `rename`/`rename_all` drift turns this red even when the fixture was
    // hand-updated to match `as_str`'s ledger names — serde's name is what
    // the socket decodes, and that is what this file pins.
    let mut wire_names: Vec<String> = VitalsField::iter()
        .map(|field| {
            serde_json::to_value(field)
                .expect("a VitalsField encodes")
                .as_str()
                .expect("a unit variant encodes as its wire name")
                .to_owned()
        })
        .collect();
    wire_names.sort_unstable();

    assert_eq!(
        pinned, wire_names,
        "the fixture's vitals_fields must be exactly the serde wire names of \
         VitalsField — a renamed or added variant is a client-facing contract change"
    );
}

#[test]
fn the_pinned_stash_ops_are_the_write_paths_accepted_set() {
    let fixture = fixture();
    let ops: Vec<&str> = fixture
        .get("stash_ops")
        .and_then(Value::as_array)
        .expect("the fixture pins the accepted stash ops")
        .iter()
        .map(|op| op.as_str().expect("op names are strings"))
        .collect();

    // Each pinned op name must be accepted by the write path's own bounds
    // validator on the stash target the dispatch routes it to — the
    // fixture's list is production-reachable, not a restated literal
    // (gh#90 F3). The payloads are the minimum each bounds arm accepts.
    for op in ops {
        let value = match op {
            "qty" => serde_json::json!({"op": "qty", "qty_delta": 1}),
            "meta" => serde_json::json!({"op": "meta", "notes": "x"}),
            other => panic!(
                "the fixture pins stash op {other:?}, which no arm of the \
                 write path accepts — update the fixture with the write path, \
                 never ahead of it"
            ),
        };
        assert!(
            validate_bounds(
                &FieldTarget::Stash {
                    party_id: 1,
                    item_name: "Rope".to_owned(),
                },
                &value,
            )
            .is_ok(),
            "the write path's bounds validator must accept the pinned op \
             {op:?} — the accepted set moved and the fixture is stale"
        );
    }

    // The deny arm: an unpinned name is rejected, so the loop above cannot
    // go green against a validator that accepts everything — and lane L3
    // cannot land `transfer`/`sell` without the fixture growing.
    assert!(
        validate_bounds(
            &FieldTarget::Stash {
                party_id: 1,
                item_name: "Rope".to_owned(),
            },
            &serde_json::json!({"op": "transfer"}),
        )
        .is_err(),
        "an op name outside the pinned set must be rejected by the bounds validator"
    );
}

#[test]
fn the_pinned_effect_ops_are_the_write_paths_accepted_set() {
    let fixture = fixture();
    let ops: Vec<&str> = fixture
        .get("effect_ops")
        .and_then(Value::as_array)
        .expect("the fixture pins the accepted effect ops")
        .iter()
        .map(|op| op.as_str().expect("op names are strings"))
        .collect();

    // Each pinned op name must be accepted by the write path's own bounds
    // validator on the target kind the dispatch routes it to — the
    // fixture's list is production-reachable, not a restated literal
    // (gh#90 F3). The payloads are the minimum each bounds arm accepts:
    // create on effect_new (name, source, targets), update on effect
    // (targets), end on effect (no fields).
    for op in ops {
        let (target, value) = match op {
            "create" => (
                FieldTarget::EffectNew { party_id: 1 },
                serde_json::json!({
                    "op": "create",
                    "name": "Bull's Strength",
                    "source_character_id": 1,
                    "targets": [3],
                }),
            ),
            "update" => (
                FieldTarget::Effect { effect_id: 41 },
                serde_json::json!({"op": "update", "targets": [3]}),
            ),
            "end" => (
                FieldTarget::Effect { effect_id: 41 },
                serde_json::json!({"op": "end"}),
            ),
            other => panic!(
                "the fixture pins effect op {other:?}, which no arm of the \
                 write path accepts — update the fixture with the write path, \
                 never ahead of it"
            ),
        };
        assert!(
            validate_bounds(&target, &value).is_ok(),
            "the write path's bounds validator must accept the pinned op \
             {op:?} — the accepted set moved and the fixture is stale"
        );
    }

    // The deny arms: an unpinned name is rejected on BOTH target kinds,
    // so the loop above cannot go green against a validator that accepts
    // everything — and the EffectNew arm cannot pass on payload shape
    // alone (round-1 F1: it hardcoded "create" and never read the op).
    assert!(
        validate_bounds(
            &FieldTarget::Effect { effect_id: 41 },
            &serde_json::json!({"op": "sunder"}),
        )
        .is_err(),
        "an op name outside the pinned set must be rejected by the bounds validator"
    );
    assert!(
        validate_bounds(
            &FieldTarget::EffectNew { party_id: 1 },
            &serde_json::json!({
                "op": "sunder",
                "name": "x",
                "source_character_id": 1,
                "targets": [3],
            }),
        )
        .is_err(),
        "an op name outside the pinned set must be rejected on effect_new too"
    );
    assert!(
        validate_bounds(
            &FieldTarget::EffectNew { party_id: 1 },
            &serde_json::json!({"name": "x", "source_character_id": 1, "targets": [3]}),
        )
        .is_err(),
        "an effect_new frame with no op key is denied by default, not shape-checked"
    );
}
