//! Integration tests for E7's snapshot builder (plan Task 4): the full
//! party live state — every versioned field of every character plus the
//! effect rows — with versions read straight from the database as the
//! oracle, party scoping, and the `snapshot_bytes` metric.

use serde_json::{Value as JsonValue, json};

use crate::auth::authz::{Actor, Role};
use crate::sync::protocol::{FieldTarget, Outcome, PartyField, SnapshotField, VitalsField};
use crate::sync::snapshot::{party_snapshot, snapshot_bytes};
use crate::sync::write::{ClientOp, apply_write};
use crate::testing;

/// The POC party (migration-seeded; the only party unless a test adds one).
async fn poc_party_id(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar("SELECT id FROM parties ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("the POC party exists")
}

/// Seed account + character + vitals row in `party_id`; return the
/// character id.
async fn seed_character_in(pool: &sqlx::PgPool, sub: &str, party_id: i64) -> i64 {
    testing::seed_account(pool, sub, "player").await;
    let character_id: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, $2, '{}', '{}') RETURNING id",
    )
    .bind(party_id)
    .bind(sub)
    .fetch_one(pool)
    .await
    .expect("seed character");
    sqlx::query("INSERT INTO character_vitals (character_id) VALUES ($1)")
        .bind(character_id)
        .execute(pool)
        .await
        .expect("seed vitals row");
    character_id
}

/// Seed one slot row and one inventory row for the character.
async fn seed_slot_and_item(pool: &sqlx::PgPool, character_id: i64) {
    sqlx::query(
        "INSERT INTO character_spell_slots (character_id, caster_key, rank, slot_index) \
         VALUES ($1, 'Wizard', 3, 0)",
    )
    .bind(character_id)
    .execute(pool)
    .await
    .expect("seed slot");
    sqlx::query(
        "INSERT INTO character_inventory_live (character_id, item_name) VALUES ($1, 'Chalk')",
    )
    .bind(character_id)
    .execute(pool)
    .await
    .expect("seed item");
}

/// Seed one effect with its target roster and ordered modifiers; return the
/// id. Modifiers are (`type`, `stat`, `value`) triples, inserted in `ord`
/// order.
async fn seed_effect(
    pool: &sqlx::PgPool,
    source_character_id: i64,
    name: &str,
    duration_note: &str,
    active: bool,
    targets: &[i64],
    modifiers: &[(&str, &str, i32)],
) -> i64 {
    let effect_id: i64 = sqlx::query_scalar(
        "INSERT INTO effects (party_id, source_character_id, name, duration_note, active) \
         SELECT party_id, id, $2, $3, $4 FROM characters WHERE id = $1 RETURNING id",
    )
    .bind(source_character_id)
    .bind(name)
    .bind(duration_note)
    .bind(active)
    .fetch_one(pool)
    .await
    .expect("seed effect");
    for target in targets {
        sqlx::query(
            "INSERT INTO effect_targets (party_id, effect_id, character_id) \
             SELECT party_id, $2, $3 FROM characters WHERE id = $1",
        )
        .bind(source_character_id)
        .bind(effect_id)
        .bind(target)
        .execute(pool)
        .await
        .expect("seed effect target");
    }
    for (ord, (modifier_type, stat, value)) in modifiers.iter().enumerate() {
        sqlx::query(
            "INSERT INTO effect_modifiers (effect_id, ord, type, stat, value) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(effect_id)
        .bind(i32::try_from(ord).expect("few modifiers"))
        .bind(modifier_type)
        .bind(stat)
        .bind(value)
        .execute(pool)
        .await
        .expect("seed modifier");
    }
    effect_id
}

/// Give the seeded rows distinct live values so the oracle proves values,
/// not defaults (`hp` 14, `temp_hp` 3, `money` 1/2/3/4, `level_adjust` 1,
/// slot used with 'Ray of Frost' prepared, Chalk at −2).
async fn shape_rows_to_distinct_values(pool: &sqlx::PgPool, character_id: i64) {
    sqlx::query(
        "UPDATE character_vitals SET hp = 14, temp_hp = 3, \
         money_pp = 1, money_gp = 2, money_sp = 3, money_cp = 4, level_adjust = 1 \
         WHERE character_id = $1",
    )
    .bind(character_id)
    .execute(pool)
    .await
    .expect("shape vitals");
    sqlx::query(
        "UPDATE character_spell_slots SET used = true, prepared_spell = 'Ray of Frost' \
         WHERE character_id = $1 AND caster_key = 'Wizard' AND rank = 3 AND slot_index = 0",
    )
    .bind(character_id)
    .execute(pool)
    .await
    .expect("shape slot");
    sqlx::query(
        "UPDATE character_inventory_live SET qty_delta = -2 \
         WHERE character_id = $1 AND item_name = 'Chalk'",
    )
    .bind(character_id)
    .execute(pool)
    .await
    .expect("shape item");
}

/// One version column from the character's vitals row (test-literal column
/// names — the `AssertSqlSafe` audit is honest, same policy as
/// src/tests/db.rs).
async fn vitals_version(pool: &sqlx::PgPool, character_id: i64, column: &str) -> i64 {
    let sql = format!("SELECT {column} FROM character_vitals WHERE character_id = $1");
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(character_id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|err| panic!("read {column}: {err}"))
}

/// One vitals expectation row (the snapshot's field/value/version shape).
fn vitals_row(
    character_id: i64,
    field: VitalsField,
    value: serde_json::Value,
    version: i64,
) -> SnapshotField {
    SnapshotField {
        field: FieldTarget::Vitals {
            character_id,
            field,
        },
        value,
        version,
    }
}

/// The expected ten non-effect rows for the shaped character: literal
/// values, versions read straight from the database — the snapshot's own
/// SELECT is never the oracle of itself.
async fn expected_base_rows(pool: &sqlx::PgPool, character_id: i64) -> Vec<SnapshotField> {
    vec![
        vitals_row(
            character_id,
            VitalsField::Hp,
            json!(14),
            vitals_version(pool, character_id, "hp_version").await,
        ),
        vitals_row(
            character_id,
            VitalsField::TempHp,
            json!(3),
            vitals_version(pool, character_id, "temp_hp_version").await,
        ),
        vitals_row(
            character_id,
            VitalsField::Money,
            json!({"pp": 1, "gp": 2, "sp": 3, "cp": 4}),
            vitals_version(pool, character_id, "money_version").await,
        ),
        vitals_row(
            character_id,
            VitalsField::LevelAdjust,
            json!(1),
            vitals_version(pool, character_id, "level_adjust_version").await,
        ),
        vitals_row(
            character_id,
            VitalsField::FocusCurrent,
            json!(0),
            vitals_version(pool, character_id, "focus_version").await,
        ),
        vitals_row(
            character_id,
            VitalsField::HeroPoints,
            json!(0),
            vitals_version(pool, character_id, "hero_points_version").await,
        ),
        vitals_row(
            character_id,
            VitalsField::Daily,
            json!({"staff_charge_rank": 0, "staff_spent": 0, "drain_used": false}),
            vitals_version(pool, character_id, "daily_version").await,
        ),
        vitals_row(
            character_id,
            VitalsField::ShieldRaised,
            json!(null),
            vitals_version(pool, character_id, "shield_raised_version").await,
        ),
        SnapshotField {
            field: FieldTarget::Slot {
                character_id,
                caster_key: "Wizard".to_owned(),
                rank: 3,
                slot_index: 0,
            },
            value: json!({"used": true, "prepared": "Ray of Frost"}),
            version: sqlx::query_scalar(
                "SELECT version FROM character_spell_slots \
                 WHERE character_id = $1 AND caster_key = 'Wizard' \
                   AND rank = 3 AND slot_index = 0",
            )
            .bind(character_id)
            .fetch_one(pool)
            .await
            .expect("slot version"),
        },
        SnapshotField {
            field: FieldTarget::Inv {
                character_id,
                item_name: "Chalk".to_owned(),
            },
            value: json!({"qty_delta": -2}),
            version: sqlx::query_scalar(
                "SELECT version FROM character_inventory_live \
                 WHERE character_id = $1 AND item_name = 'Chalk'",
            )
            .bind(character_id)
            .fetch_one(pool)
            .await
            .expect("inv version"),
        },
    ]
}

/// One expected effect row: contract §3 value shape with the given
/// modifiers, id on the target.
fn effect_row(
    effect_id: i64,
    source_character_id: i64,
    version: i64,
    name: &str,
    duration_note: &str,
    active: bool,
    modifiers: &JsonValue,
) -> SnapshotField {
    SnapshotField {
        field: FieldTarget::Effect { effect_id },
        value: json!({
            "name": name,
            "source_character_id": source_character_id,
            "targets": [source_character_id],
            "modifiers": modifiers,
            "duration_note": duration_note,
            "active": active,
            // E8 amended the row shape: the badge-chip flag rides the value
            // (false for these hand-seeded rows).
            "tracked_manually": false
        }),
        version,
    }
}

/// The version of one seeded effect, from the database.
async fn effect_version(pool: &sqlx::PgPool, effect_id: i64) -> i64 {
    sqlx::query_scalar("SELECT version FROM effects WHERE id = $1")
        .bind(effect_id)
        .fetch_one(pool)
        .await
        .expect("effect version")
}

#[tokio::test]
async fn a_snapshot_carries_every_versioned_field_with_its_db_version() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = poc_party_id(&pool).await;
    let character_id = seed_character_in(&pool, "sub-snap-owner", party_id).await;
    seed_slot_and_item(&pool, character_id).await;
    shape_rows_to_distinct_values(&pool, character_id).await;
    let effect_bless = seed_effect(
        &pool,
        character_id,
        "Bless",
        "10 rounds",
        true,
        &[character_id],
        &[("status", "attack", 1)],
    )
    .await;
    let effect_fatigued = seed_effect(
        &pool,
        character_id,
        "Fatigued",
        "",
        false,
        &[character_id],
        &[
            ("circumstance", "all_checks", 0),
            ("untyped", "fort_save", -2),
        ],
    )
    .await;

    let fields = party_snapshot(&pool, party_id)
        .await
        .expect("snapshot builds");

    let mut expected = expected_base_rows(&pool, character_id).await;
    expected.push(effect_row(
        effect_bless,
        character_id,
        effect_version(&pool, effect_bless).await,
        "Bless",
        "10 rounds",
        true,
        &json!([{"type": "status", "stat": "attack", "value": 1}]),
    ));
    expected.push(effect_row(
        effect_fatigued,
        character_id,
        effect_version(&pool, effect_fatigued).await,
        "Fatigued",
        "",
        false,
        &json!([
            {"type": "circumstance", "stat": "all_checks", "value": 0},
            {"type": "untyped", "stat": "fort_save", "value": -2}
        ]),
    ));
    // The E12 party field rides every snapshot (the designation is versioned
    // party state, not a character's row).
    let (qm_character, qm_version): (Option<i64>, i64) = sqlx::query_as(
        "SELECT quartermaster_character_id, quartermaster_version FROM parties WHERE id = $1",
    )
    .bind(party_id)
    .fetch_one(&pool)
    .await
    .expect("party row");
    expected.push(SnapshotField {
        field: FieldTarget::Party {
            party_id,
            field: PartyField::Quartermaster,
        },
        value: json!({"quartermaster_character_id": qm_character}),
        version: qm_version,
    });

    assert_eq!(
        fields.len(),
        expected.len(),
        "no extra rows beyond the field set: {fields:?}"
    );
    for want in &expected {
        assert!(
            fields.contains(want),
            "snapshot is missing {want:?}\nin: {fields:?}"
        );
    }
    testing::drop_test_db(pool, "snapshot_full").await;
}

/// Does this target address `character_id`'s rows? Effects are keyed by
/// effect id, so their scoping is judged via the value's
/// `source_character_id`, not the target.
fn targets_character(target: &FieldTarget, character_id: i64) -> bool {
    match target {
        FieldTarget::Vitals {
            character_id: owned,
            ..
        }
        | FieldTarget::Slot {
            character_id: owned,
            ..
        }
        | FieldTarget::Inv {
            character_id: owned,
            ..
        } => *owned == character_id,
        // Effects and the E12 party targets address no character's rows —
        // the quartermaster designation names a character but does not
        // address that character's rows.
        FieldTarget::Effect { .. }
        | FieldTarget::EffectNew { .. }
        | FieldTarget::StashNew { .. }
        | FieldTarget::Stash { .. }
        | FieldTarget::Bank { .. }
        | FieldTarget::Party { .. }
        | FieldTarget::Claim { .. } => false,
    }
}

#[tokio::test]
async fn a_snapshot_contains_only_the_party_s_own_rows() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = poc_party_id(&pool).await;
    let own_id = seed_character_in(&pool, "sub-snap-own", party_id).await;
    seed_slot_and_item(&pool, own_id).await;

    // A second party with its own character, slot, item, and effect.
    let other_party: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('Other Party') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("second party");
    let other_id = seed_character_in(&pool, "sub-snap-other", other_party).await;
    seed_slot_and_item(&pool, other_id).await;
    seed_effect(
        &pool,
        other_id,
        "Rage",
        "1 minute",
        true,
        &[other_id],
        &[("status", "attack", 2)],
    )
    .await;

    let own = party_snapshot(&pool, party_id).await.expect("own snapshot");
    assert_eq!(
        own.len(),
        8 + 1 + 1 + 1,
        "own party: 8 vitals + 1 slot + 1 item + the party settings field, no effects"
    );
    assert!(
        own.iter()
            .all(|row| !targets_character(&row.field, other_id)),
        "no other-party targets in own snapshot: {own:?}"
    );

    let other = party_snapshot(&pool, other_party)
        .await
        .expect("other snapshot");
    assert_eq!(
        other.len(),
        8 + 1 + 1 + 1 + 1,
        "other party: 8 vitals + 1 slot + 1 item + 1 effect + the party settings field"
    );
    assert!(
        other
            .iter()
            .all(|row| !targets_character(&row.field, own_id)),
        "no own-party targets in other snapshot: {other:?}"
    );
    let other_effect = other
        .iter()
        .find(|row| matches!(row.field, FieldTarget::Effect { .. }))
        .expect("the other party's effect row");
    assert_eq!(
        other_effect.value.get("source_character_id"),
        Some(&json!(other_id)),
        "the effect belongs to the other party's character"
    );
    testing::drop_test_db(pool, "snapshot_scoped").await;
}

#[tokio::test]
async fn snapshot_bytes_is_the_serialized_length_of_the_rows() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = poc_party_id(&pool).await;
    let character_id = seed_character_in(&pool, "sub-snap-bytes", party_id).await;
    seed_slot_and_item(&pool, character_id).await;
    let fields = party_snapshot(&pool, party_id)
        .await
        .expect("snapshot builds");

    let serialized = serde_json::to_vec(&fields).expect("rows serialize").len() as u64;
    assert!(serialized > 0, "a seeded snapshot is non-empty");
    assert_eq!(
        snapshot_bytes(&fields).expect("bytes"),
        serialized,
        "the metric is the serialized length"
    );
    testing::drop_test_db(pool, "snapshot_bytes").await;
}

#[tokio::test]
async fn an_empty_party_yields_only_the_party_settings_row() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let empty_party: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('Empty Party') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("empty party");

    let fields = party_snapshot(&pool, empty_party)
        .await
        .expect("snapshot builds");
    // No characters, no stash, no bank row, no claims — but the party's own
    // settings field is versioned party state and rides every snapshot (E12).
    assert_eq!(fields.len(), 1, "only the party settings field");
    let party_field = fields.first().expect("one row asserted above");
    assert!(
        matches!(
            &party_field.field,
            FieldTarget::Party {
                field: PartyField::Quartermaster,
                ..
            }
        ),
        "the one row is the quartermaster field: {:?}",
        party_field.field
    );
    assert_eq!(
        party_field.value,
        json!({"quartermaster_character_id": null}),
        "no designation"
    );
    assert_eq!(
        snapshot_bytes(&fields).expect("bytes"),
        serde_json::to_vec(&fields).expect("rows serialize").len() as u64,
    );
    testing::drop_test_db(pool, "snapshot_empty").await;
}

// E12 (plan Task 2, design D2): the snapshot carries the stash, bank, and
// claim sections with the same shapes the write path fans out — one shape,
// two riders (D7).

/// Seed the stash row (mint), the bank balance (credit), and the
/// quartermaster designation through the production write path; return the
/// mint's stash-row version and the credit's bank-row version.
async fn seed_inventory_rows(
    pool: &sqlx::PgPool,
    actor: &Actor,
    party_id: i64,
    character_id: i64,
) -> (i64, i64) {
    // The bank is quartermaster-only (D4); designate our character. The flip
    // is CAS on `quartermaster_version` — read the current base first.
    let base: i64 = sqlx::query_scalar("SELECT quartermaster_version FROM parties WHERE id = $1")
        .bind(party_id)
        .fetch_one(pool)
        .await
        .expect("quartermaster version");
    let designation = apply_write(
        pool,
        actor,
        party_id,
        ClientOp {
            op_id: "snap-designate".to_owned(),
            target: FieldTarget::Party {
                party_id,
                field: PartyField::Quartermaster,
            },
            base_version: base,
            value: json!({"character_id": character_id}),
        },
    )
    .await
    .expect("designation applies");
    assert_eq!(designation.outcome, Outcome::Applied);

    // The mint creates the stash row and its outside→stash claim; the bank
    // delta mints the balance row (no claim — that is Task 3's write).
    let mint = apply_write(
        pool,
        actor,
        party_id,
        ClientOp {
            op_id: "snap-mint".to_owned(),
            target: FieldTarget::StashNew { party_id },
            base_version: 0,
            value: json!({"item_name": "Rope", "quantity": 4}),
        },
    )
    .await
    .expect("mint applies");
    assert_eq!(mint.outcome, Outcome::Applied);
    let credit = apply_write(
        pool,
        actor,
        party_id,
        ClientOp {
            op_id: "snap-bank".to_owned(),
            target: FieldTarget::Bank { party_id },
            base_version: 0,
            value: json!({"gp_delta": 10, "sp_delta": 5, "cp_delta": 1}),
        },
    )
    .await
    .expect("bank credit applies");
    assert_eq!(credit.outcome, Outcome::Applied);

    (
        mint.version.expect("the mint ack carries a version"),
        credit.version.expect("the credit ack carries a version"),
    )
}

#[tokio::test]
async fn a_snapshot_carries_the_stash_bank_and_claim_sections() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) =
        crate::sync::test_helpers::seed_member(&pool, "sub-snap-stash").await;
    let actor = Actor {
        sub: "sub-snap-stash".to_owned(),
        role: Role::Player,
    };
    let (mint_version, credit_version) =
        seed_inventory_rows(&pool, &actor, party_id, character_id).await;

    let fields = party_snapshot(&pool, party_id)
        .await
        .expect("snapshot builds");

    // The stash section: one row, the D2 value shape, the minted version.
    let stash: Vec<&SnapshotField> = fields
        .iter()
        .filter(|field| {
            matches!(
                field.field,
                FieldTarget::Stash {
                    party_id: pid,
                    ..
                } if pid == party_id
            )
        })
        .collect();
    assert_eq!(stash.len(), 1, "one stash row: {stash:?}");
    let row = stash.first().expect("one row asserted above");
    assert_eq!(
        row.value,
        json!({"item_name": "Rope", "quantity": 4, "bulk": null, "notes": ""}),
        "the stash snapshot value is the D2 shape"
    );
    assert_eq!(row.version, mint_version);

    // The bank section: the minted balance row, gp/sp/cp in order.
    let bank: Vec<&SnapshotField> = fields
        .iter()
        .filter(|field| {
            matches!(
                field.field,
                FieldTarget::Bank {
                    party_id: pid
                } if pid == party_id
            )
        })
        .collect();
    assert_eq!(bank.len(), 1, "one bank row: {bank:?}");
    let balance = bank.first().expect("one row asserted above");
    assert_eq!(balance.value, json!({"gp": 10, "sp": 5, "cp": 1}));
    assert_eq!(balance.version, credit_version);

    // The claim section: one append-only row, id riding the version slot.
    let claims: Vec<&SnapshotField> = fields
        .iter()
        .filter(|field| matches!(field.field, FieldTarget::Claim { .. }))
        .collect();
    assert_eq!(claims.len(), 1, "the mint appends one claim: {claims:?}");
    let claim = claims.first().expect("one claim asserted above");
    let FieldTarget::Claim { claim_id } = claim.field else {
        panic!("the claim section targets Claim, got {:?}", claim.field);
    };
    assert_eq!(
        claim.version, claim_id,
        "the claim id rides the version slot"
    );
    assert_eq!(claim.value.get("origin"), Some(&json!("outside")));
    assert_eq!(claim.value.get("destination"), Some(&json!("stash")));
    assert_eq!(claim.value.get("item_name"), Some(&json!("Rope")));
    assert_eq!(claim.value.get("quantity"), Some(&json!(4)));
    testing::drop_test_db(pool, "snapshot_stash_bank_claim").await;
}

// E6: the spell-economy fields ride the snapshot like every vitals field.
#[tokio::test]
async fn the_snapshot_carries_the_spell_economy_fields() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = poc_party_id(&pool).await;
    let character_id = seed_character_in(&pool, "sub-snap-e6", party_id).await;

    let fields = party_snapshot(&pool, party_id).await.expect("snapshot");
    let by_field = |name: &str| {
        fields
            .iter()
            .find(
                |f| matches!(&f.field, FieldTarget::Vitals { field, .. } if field.as_str() == name),
            )
            .unwrap_or_else(|| panic!("{name} rides the snapshot"))
    };
    let focus = by_field("focus_current");
    assert_eq!(focus.value, serde_json::json!(0), "the up default");
    assert!(focus.version > 0, "versioned like every field");
    assert_eq!(by_field("hero_points").value, serde_json::json!(0));
    let daily = by_field("daily");
    assert_eq!(
        daily.value,
        serde_json::json!({"staff_charge_rank": 0, "staff_spent": 0, "drain_used": false}),
    );
    assert_eq!(focus.field, by_field("focus_current").field);

    let _ = character_id;
    testing::drop_test_db(pool, "snapshot_spell_economy").await;
}
