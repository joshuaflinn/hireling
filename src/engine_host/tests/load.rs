//! Task 6 load tests: party rows → engine `ActiveEffect[]` (targets,
//! ord-ordered modifiers, version, the new corpus columns) against a real
//! Postgres via the shared `testing::test_pool` harness. A corrupt stored
//! stat is loud — a silently skipped row would lie to every derived total
//! downstream (provenance completeness, SC-4).

use sqlx::{PgPool, Row as _};

use crate::engine_host::load::party_effects;
use crate::testing;
use hireling_engine::model::{ActiveEffect, Modifier};
use hireling_engine::vocab::{ModifierType, Stat};

/// Execute one statement, panicking with context when it fails (same policy
/// as `src/tests/migrations.rs`: every string is test-authored — no user
/// input — so the [`sqlx::AssertSqlSafe`] audit is honest).
async fn exec(pool: &PgPool, sql: &str, label: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap_or_else(|err| panic!("{label}: {err}"));
}

async fn one_i64(pool: &PgPool, sql: &str, label: &str) -> i64 {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|err| panic!("{label}: {err}"))
        .get::<i64, _>(0)
}

/// One account + party + two characters (imported, with vitals).
/// Returns `(party_id, character_ids)`.
async fn seed_party(pool: &PgPool, sub: &str) -> (i64, [i64; 2]) {
    exec(
        pool,
        &format!(
            "INSERT INTO accounts (sub, username, display_name, role)
             VALUES ('{sub}', '{sub}', 'Display {sub}', 'player')"
        ),
        "seed account",
    )
    .await;
    let party = one_i64(
        pool,
        "INSERT INTO parties (name) VALUES ('E8 load') RETURNING id",
        "seed party",
    )
    .await;
    // owner_sub is unique on characters — one account per character.
    let mut ids = [0_i64; 2];
    for (index, id) in ids.iter_mut().enumerate() {
        let owner = format!("{sub}-{index}");
        exec(
            pool,
            &format!(
                "INSERT INTO accounts (sub, username, display_name, role)
                 VALUES ('{owner}', '{owner}', 'Display {owner}', 'player')"
            ),
            &format!("seed account {index}"),
        )
        .await;
        *id = one_i64(
            pool,
            &format!(
                "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet)
                 VALUES ({party}, '{owner}', '{{\"success\":true}}', '{{\"level\":5}}')
                 RETURNING id"
            ),
            &format!("seed character {index}"),
        )
        .await;
        exec(
            pool,
            &format!(
                "INSERT INTO character_vitals (character_id, hp, temp_hp) VALUES ({}, 25, 3)",
                *id
            ),
            "seed vitals",
        )
        .await;
    }
    (party, ids)
}

/// Insert one effect with two targets and two modifiers; returns its id.
async fn seed_full_effect(pool: &PgPool, party: i64, targets: [i64; 2]) -> i64 {
    let effect = one_i64(
        pool,
        &format!(
            "INSERT INTO effects (party_id, source_character_id, name, duration_note)
             VALUES ({party}, {}, 'Frightened', '3 rounds') RETURNING id",
            targets.first().expect("two targets seeded")
        ),
        "seed effect",
    )
    .await;
    for character_id in targets {
        exec(
            pool,
            &format!(
                "INSERT INTO effect_targets (party_id, effect_id, character_id)
                 VALUES ({party}, {effect}, {character_id})"
            ),
            "seed target",
        )
        .await;
    }
    for (ord, (modifier_type, stat_value, modifier_value)) in [
        ("status", "all_checks_and_dcs", -2_i32),
        ("circumstance", "ac", 1_i32),
    ]
    .into_iter()
    .enumerate()
    {
        exec(
            pool,
            &format!(
                "INSERT INTO effect_modifiers (effect_id, ord, type, stat, value)
                 VALUES ({effect}, {ord}, '{modifier_type}', '{stat_value}', {modifier_value})"
            ),
            "seed modifier",
        )
        .await;
    }
    effect
}

fn stat(text: &str) -> hireling_engine::vocab::StatName {
    Stat::parse(text)
        .expect("test stat is canonical")
        .stat_name()
}

#[tokio::test]
async fn party_effects_load_with_targets_and_ordered_modifiers() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party, [alpha, beta]) = seed_party(&pool, "load-full").await;
    let effect = seed_full_effect(&pool, party, [alpha, beta]).await;

    let effects = party_effects(&pool, party)
        .await
        .expect("party effects load");
    assert_eq!(effects.len(), 1, "exactly the seeded effect");
    let loaded: &ActiveEffect = effects.first().expect("one effect loads");
    assert_eq!(loaded.effect_id, effect);
    assert_eq!(loaded.name, "Frightened");
    assert_eq!(loaded.source_character_id, alpha);
    assert_eq!(loaded.targets, vec![alpha, beta], "targets, id order");
    assert_eq!(
        loaded.modifiers,
        vec![
            Modifier {
                modifier_type: ModifierType::Status,
                stat: stat("all_checks_and_dcs"),
                value: -2,
            },
            Modifier {
                modifier_type: ModifierType::Circumstance,
                stat: stat("ac"),
                value: 1,
            },
        ],
        "modifiers in ord order, stats parsed to the closed vocabulary"
    );
    assert_eq!(loaded.duration_note, "3 rounds");
    assert!(loaded.active);
    assert!(!loaded.tracked_manually, "hand-built effect: plain row");
    assert!(loaded.version >= 1, "version carried from the row");
    testing::drop_test_db(pool, "e8_load_full").await;
}

#[tokio::test]
async fn ended_effects_stay_queryable_and_load() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party, [alpha, _]) = seed_party(&pool, "load-ended").await;
    let effect = one_i64(
        &pool,
        &format!(
            "INSERT INTO effects (party_id, source_character_id, name, active)
             VALUES ({party}, {alpha}, 'Bless', false) RETURNING id"
        ),
        "seed ended effect",
    )
    .await;

    let effects = party_effects(&pool, party).await.expect("load");
    assert_eq!(effects.len(), 1, "ended is state, not deletion (FR-15)");
    let loaded = effects.first().expect("one effect loads");
    assert_eq!(loaded.effect_id, effect);
    assert!(!loaded.active);
    assert!(loaded.targets.is_empty());
    assert!(loaded.modifiers.is_empty());
    testing::drop_test_db(pool, "e8_load_ended").await;
}

#[tokio::test]
async fn corpus_columns_carry_through() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party, [alpha, _]) = seed_party(&pool, "load-corpus").await;
    let corpus = one_i64(
        &pool,
        "INSERT INTO corpus_entries (kind, name, lane, data)
         VALUES ('condition', 'Frightened', 'core', '{}') RETURNING id",
        "seed corpus row",
    )
    .await;
    exec(
        &pool,
        &format!(
            "INSERT INTO effects (party_id, source_character_id, name, tracked_manually, corpus_entry_id)
             VALUES ({party}, {alpha}, 'Frightened', true, {corpus})"
        ),
        "seed display-only condition effect",
    )
    .await;

    let effects = party_effects(&pool, party).await.expect("load");
    let loaded = effects.first().expect("one effect loads");
    assert!(loaded.tracked_manually, "badge chip flag frozen at apply");
    testing::drop_test_db(pool, "e8_load_corpus").await;
}

#[tokio::test]
async fn an_empty_party_loads_empty() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party, _) = seed_party(&pool, "load-empty").await;
    let effects = party_effects(&pool, party).await.expect("load");
    assert!(effects.is_empty());
    testing::drop_test_db(pool, "e8_load_empty").await;
}

#[tokio::test]
async fn a_corrupt_stored_stat_is_loud() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party, [alpha, _]) = seed_party(&pool, "load-corrupt").await;
    let effect = one_i64(
        &pool,
        &format!(
            "INSERT INTO effects (party_id, source_character_id, name)
             VALUES ({party}, {alpha}, 'Corrupt') RETURNING id"
        ),
        "seed effect",
    )
    .await;
    exec(
        &pool,
        &format!(
            "INSERT INTO effect_modifiers (effect_id, ord, type, stat, value)
             VALUES ({effect}, 0, 'status', 'initiative', -1)"
        ),
        "seed corrupt modifier",
    )
    .await;
    // The effect_modifiers CHECK only bounds `type` and ord — stat text is
    // E8's enforcement (E2's own comment). A row that predates validation
    // (or a hand edit) must fail the load loudly, never vanish silently.
    assert!(
        party_effects(&pool, party).await.is_err(),
        "the load must fail loudly on a stat outside the closed vocabulary"
    );
    testing::drop_test_db(pool, "e8_load_corrupt").await;
}
