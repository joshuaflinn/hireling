//! Integration tests for E7's snapshot builder (plan Task 4): the full
//! party live state — every versioned field of every character plus the
//! effect rows — with versions read straight from the database as the
//! oracle, party scoping, and the `snapshot_bytes` metric.

use serde_json::{Value as JsonValue, json};

use crate::sync::protocol::{FieldTarget, SnapshotField, VitalsField};
use crate::sync::snapshot::{party_snapshot, snapshot_bytes};
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

/// The expected six non-effect rows for the shaped character: literal
/// values, versions read straight from the database — the snapshot's own
/// SELECT is never the oracle of itself.
async fn expected_base_rows(pool: &sqlx::PgPool, character_id: i64) -> Vec<SnapshotField> {
    vec![
        SnapshotField {
            field: FieldTarget::Vitals {
                character_id,
                field: VitalsField::Hp,
            },
            value: json!(14),
            version: vitals_version(pool, character_id, "hp_version").await,
        },
        SnapshotField {
            field: FieldTarget::Vitals {
                character_id,
                field: VitalsField::TempHp,
            },
            value: json!(3),
            version: vitals_version(pool, character_id, "temp_hp_version").await,
        },
        SnapshotField {
            field: FieldTarget::Vitals {
                character_id,
                field: VitalsField::Money,
            },
            value: json!({"pp": 1, "gp": 2, "sp": 3, "cp": 4}),
            version: vitals_version(pool, character_id, "money_version").await,
        },
        SnapshotField {
            field: FieldTarget::Vitals {
                character_id,
                field: VitalsField::LevelAdjust,
            },
            value: json!(1),
            version: vitals_version(pool, character_id, "level_adjust_version").await,
        },
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
            "active": active
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
        FieldTarget::Effect { .. } | FieldTarget::EffectNew { .. } => false,
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
        4 + 1 + 1,
        "own party: 4 vitals + 1 slot + 1 item, no effects"
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
        4 + 1 + 1 + 1,
        "other party: 4 vitals + 1 slot + 1 item + 1 effect"
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
async fn an_empty_party_yields_an_empty_snapshot() {
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
    assert!(fields.is_empty(), "no characters, no rows");
    assert_eq!(
        snapshot_bytes(&fields).expect("bytes"),
        serde_json::to_vec(&fields)
            .expect("empty rows serialize")
            .len() as u64,
    );
    testing::drop_test_db(pool, "snapshot_empty").await;
}
