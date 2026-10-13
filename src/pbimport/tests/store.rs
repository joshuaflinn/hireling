//! Integration tests for the import store (FR-7/8/9/12) against compose
//! Postgres: first-import seeding, re-import preservation with row versions
//! pinned, concurrent double-import serialization, the six-account roster,
//! and audit rows per attempt.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use sqlx::Row as _;
use tower::ServiceExt as _;

use super::run_import;
use crate::auth::authz::{Actor, Role};
use crate::auth::oidc;
use crate::testing::{self, PLAYER_SUB};

fn player(sub: &str) -> Actor {
    Actor {
        sub: sub.to_owned(),
        role: Role::Player,
    }
}

/// `POST /api/characters/import` over the configured router — the
/// production path that owns both audit rows (allowed and denied), so an
/// audit test must drive it rather than call the recorder (gh#31).
fn post_import(session: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/characters/import")
        .header("cookie", format!("{}={}", oidc::SESSION_COOKIE, session))
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .expect("request builds")
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("response is JSON")
}

/// Seed the account row the characters FK requires (store tests bypass the
/// login path that upserts accounts).
async fn seed_actor(pool: &sqlx::PgPool, sub: &str) {
    sqlx::query(
        "INSERT INTO accounts (sub, username, display_name, role) \
         VALUES ($1, $1, $1, 'player') ON CONFLICT (sub) DO NOTHING",
    )
    .bind(sub)
    .execute(pool)
    .await
    .expect("seed account");
}

async fn pool_or_skip() -> Option<sqlx::PgPool> {
    testing::test_pool().await
}

#[tokio::test]
async fn first_import_creates_the_character_seeded_and_ready() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    seed_actor(&pool, PLAYER_SUB).await;
    let body = crate::pbimport::fixtures::reference_export();
    let outcome = run_import(&pool, &player(PLAYER_SUB), Some("req-first"), body)
        .await
        .expect("first import succeeds");

    assert!(outcome.character.first_import, "this was a first import");
    assert_eq!(outcome.character.name, "Lorum Ipsum");
    assert_eq!(outcome.character.level, 3);
    assert_eq!(outcome.character.owner, PLAYER_SUB);
    assert!(outcome.diff.first_import, "first-import diff");
    assert!(
        outcome.advisory.skipped_fields == 0,
        "fixture has no unknowns"
    );

    // Ownership + party binding (FR-7).
    let character_row = sqlx::query(
        "SELECT c.party_id, c.payload_raw, p.name AS party_name \
         FROM characters c JOIN parties p ON p.id = c.party_id \
         WHERE c.owner_sub = $1",
    )
    .bind(PLAYER_SUB)
    .fetch_one(&pool)
    .await
    .expect("character row");
    assert_eq!(character_row.get::<String, _>("party_name"), "POC Party");
    assert_eq!(
        character_row.get::<String, _>("payload_raw"),
        body,
        "payload_raw is the body byte-verbatim"
    );

    // Vitals seeded (FR-9): HP at the PF2e max (32 — CON counts at every
    // level, contract §3.3), focus at the export's full pool (1 — daily
    // preparations refill it), money from the export.
    let vitals = sqlx::query(
        "SELECT hp, temp_hp, focus_current, money_gp, money_sp, money_cp, money_pp \
         FROM character_vitals WHERE character_id = $1",
    )
    .bind(outcome.character.id)
    .fetch_one(&pool)
    .await
    .expect("vitals row");
    assert_eq!(vitals.get::<i32, _>("hp"), 32, "HP starts at max");
    assert_eq!(vitals.get::<i32, _>("temp_hp"), 0, "temp HP starts at 0");
    assert_eq!(
        vitals.get::<i32, _>("focus_current"),
        1,
        "focus boots at the export's pool, not empty"
    );
    assert_eq!(vitals.get::<i32, _>("money_gp"), 24, "gp");
    assert_eq!(vitals.get::<i32, _>("money_sp"), 2, "sp");
    assert_eq!(vitals.get::<i32, _>("money_cp"), 4, "cp");
    assert_eq!(vitals.get::<i32, _>("money_pp"), 0, "pp");

    // Every layout slot materialized with export prep (FR-12).
    let slots = sqlx::query(
        "SELECT caster_key, rank, slot_index, used, prepared_spell \
         FROM character_spell_slots WHERE character_id = $1",
    )
    .bind(outcome.character.id)
    .fetch_all(&pool)
    .await
    .expect("slot rows");
    assert_eq!(slots.len(), 14, "6+4+3 wizard slots + 1 innate cantrip");
    let toads = slots
        .iter()
        .find(|row| {
            row.get::<String, _>("caster_key") == "Wizard"
                && row.get::<i32, _>("rank") == 1
                && row.get::<i32, _>("slot_index") == 0
        })
        .expect("wizard rank-1 slot 0");
    assert_eq!(
        toads.get::<Option<String>, _>("prepared_spell").as_deref(),
        Some("500 Toads")
    );
    assert!(
        slots.iter().all(|row| !row.get::<bool, _>("used")),
        "every slot seeds unused"
    );
    let gnome = slots
        .iter()
        .find(|row| row.get::<String, _>("caster_key") == "Wellspring Gnome")
        .expect("the innate cantrip seeds");
    assert_eq!(
        gnome.get::<Option<String>, _>("prepared_spell"),
        None,
        "innate block prepares nothing"
    );
    // The audit trail: one allowed row with the character target (FR-16).
    let (event, target, outcome_code): (String, String, String) = sqlx::query_as(
        "SELECT event, target, outcome FROM audit_events WHERE event = 'character_import'",
    )
    .fetch_one(&pool)
    .await
    .expect("import audit row");
    assert_eq!(event, "character_import");
    assert_eq!(target, format!("character:{}", outcome.character.id));
    assert_eq!(outcome_code, "allowed");
    testing::drop_test_db(pool, "store_first_import").await;
}

/// First import + session-like mutations: HP dropped to 7, one slot spent
/// and prepped to "Fear", a −2 chalk delta.
async fn imported_with_mutations() -> Option<(sqlx::PgPool, i64)> {
    let pool = pool_or_skip().await?;
    seed_actor(&pool, PLAYER_SUB).await;
    let body = crate::pbimport::fixtures::reference_export();
    let first = run_import(&pool, &player(PLAYER_SUB), None, body)
        .await
        .expect("first import");
    let character_id = first.character.id;
    sqlx::query("UPDATE character_vitals SET hp = 7 WHERE character_id = $1")
        .bind(character_id)
        .execute(&pool)
        .await
        .expect("hp drop");
    sqlx::query(
        "UPDATE character_spell_slots SET used = true, prepared_spell = 'Fear' \
         WHERE character_id = $1 AND caster_key = 'Wizard' AND rank = 1 AND slot_index = 1",
    )
    .bind(character_id)
    .execute(&pool)
    .await
    .expect("slot spend + swap");
    sqlx::query(
        "INSERT INTO character_inventory_live (character_id, item_name, qty_delta) \
         VALUES ($1, 'Chalk', -2)",
    )
    .bind(character_id)
    .execute(&pool)
    .await
    .expect("inventory delta");
    Some((pool, character_id))
}

#[tokio::test]
async fn re_import_replaces_the_sheet_and_touches_zero_live_rows() {
    let Some((pool, character_id)) = imported_with_mutations().await else {
        return;
    };
    let body = crate::pbimport::fixtures::reference_export();

    let versions_before: Vec<(i64,)> =
        sqlx::query_as("SELECT hp_version FROM character_vitals WHERE character_id = $1")
            .bind(character_id)
            .fetch_all(&pool)
            .await
            .expect("vitals version");
    let slot_versions_before: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM character_spell_slots WHERE character_id = $1 ORDER BY caster_key, rank, slot_index",
    )
    .bind(character_id)
    .fetch_all(&pool)
    .await
    .expect("slot versions");

    // Re-import the identical export (FR-13).
    let second = run_import(&pool, &player(PLAYER_SUB), None, body)
        .await
        .expect("re-import");
    assert!(!second.character.first_import, "this was a re-import");
    assert_eq!(second.character.id, character_id, "same character (FR-8)");
    // The prep swap the test made is a real divergence: live "Fear" vs the
    // export's "Befuddle" — named, and live wins. Everything else is quiet.
    assert_eq!(second.diff.kept_unmatched.len(), 0, "nothing vanished");
    assert_eq!(second.diff.seeded.len(), 0, "nothing new");
    assert_eq!(
        second.diff.prep_divergence.len(),
        1,
        "exactly the prep swap"
    );
    let divergence = second
        .diff
        .prep_divergence
        .first()
        .expect("divergence present");
    assert_eq!(divergence.caster_key, "Wizard");
    assert_eq!(divergence.rank, 1);
    assert_eq!(divergence.slot_index, 1);
    assert_eq!(divergence.live.as_deref(), Some("Fear"));
    assert_eq!(divergence.export.as_deref(), Some("Befuddle"));

    let (hp,): (i32,) = sqlx::query_as("SELECT hp FROM character_vitals WHERE character_id = $1")
        .bind(character_id)
        .fetch_one(&pool)
        .await
        .expect("hp");
    assert_eq!(hp, 7, "live HP preserved (FR-10)");
    let versions_after: Vec<(i64,)> =
        sqlx::query_as("SELECT hp_version FROM character_vitals WHERE character_id = $1")
            .bind(character_id)
            .fetch_all(&pool)
            .await
            .expect("vitals version after");
    assert_eq!(versions_before, versions_after, "vitals versions untouched");

    let slot_versions_after: Vec<(i64,)> = sqlx::query_as(
        "SELECT version FROM character_spell_slots WHERE character_id = $1 ORDER BY caster_key, rank, slot_index",
    )
    .bind(character_id)
    .fetch_all(&pool)
    .await
    .expect("slot versions after");
    assert_eq!(
        slot_versions_before, slot_versions_after,
        "slot rows untouched"
    );

    let slot = sqlx::query(
        "SELECT used, prepared_spell FROM character_spell_slots \
         WHERE character_id = $1 AND caster_key = 'Wizard' AND rank = 1 AND slot_index = 1",
    )
    .bind(character_id)
    .fetch_one(&pool)
    .await
    .expect("the spent slot");
    assert!(slot.get::<bool, _>("used"), "used preserved");
    assert_eq!(
        slot.get::<Option<String>, _>("prepared_spell").as_deref(),
        Some("Fear"),
        "live prep wins over export prep"
    );
    let (chalk,): (i32,) =
        sqlx::query_as("SELECT qty_delta FROM character_inventory_live WHERE character_id = $1 AND item_name = 'Chalk'")
            .bind(character_id)
            .fetch_one(&pool)
            .await
            .expect("chalk delta");
    assert_eq!(chalk, -2, "inventory delta preserved");

    testing::drop_test_db(pool, "store_re_import").await;
}

#[tokio::test]
async fn re_import_after_drift_names_what_changed() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    seed_actor(&pool, PLAYER_SUB).await;
    let body = crate::pbimport::fixtures::reference_export();
    let first = run_import(&pool, &player(PLAYER_SUB), None, body)
        .await
        .expect("first import");

    // A modified export: shrink Wizard rank 2 to one slot (was 3).
    let mut doc: serde_json::Value =
        serde_json::from_str(crate::pbimport::fixtures::reference_export()).expect("fixture");
    let per_day = doc
        .get_mut("build")
        .and_then(|build| build.get_mut("spellCasters"))
        .and_then(|casters| casters.get_mut(0))
        .and_then(|caster| caster.get_mut("perDay"))
        .and_then(|per_day| per_day.as_array_mut())
        .expect("perDay array");
    if let Some(slot) = per_day.get_mut(2) {
        *slot = serde_json::json!(1);
    }
    let shrunk = serde_json::to_string(&doc).expect("serializes");

    sqlx::query(
        "UPDATE character_spell_slots SET used = true, prepared_spell = 'Illusory Creature' \
         WHERE character_id = $1 AND caster_key = 'Wizard' AND rank = 2 AND slot_index = 2",
    )
    .bind(first.character.id)
    .execute(&pool)
    .await
    .expect("spend the doomed slot");

    let outcome = run_import(&pool, &player(PLAYER_SUB), None, &shrunk)
        .await
        .expect("re-import with a shrunk rank");

    // The beyond-layout slots were kept, verbatim, and named (FR-11):
    // new layout grants rank 2 index 0 only; live rows at indices 1 and 2
    // are kept — index 2 was set used=true before the re-import.
    let kept = &outcome.diff.kept_unmatched;
    assert_eq!(kept.len(), 2, "indices 1 and 2 are beyond the new layout");
    assert!(
        kept.iter()
            .any(|entry| entry.slot_index == Some(1) && entry.used == Some(false)),
        "index 1 kept unused"
    );
    assert!(
        kept.iter()
            .any(|entry| entry.slot_index == Some(2) && entry.used == Some(true)),
        "index 2 kept with its spent state"
    );
    assert!(
        outcome.diff.seeded.is_empty(),
        "a shrunk rank seeds nothing"
    );

    // Nothing is ever dropped: rank-2 rows are all three still there.
    let (rank2_rows,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM character_spell_slots \
         WHERE character_id = $1 AND caster_key = 'Wizard' AND rank = 2",
    )
    .bind(first.character.id)
    .fetch_one(&pool)
    .await
    .expect("rank-2 count");
    assert_eq!(rank2_rows, 3, "kept rows are never deleted (FR-11)");
    testing::drop_test_db(pool, "store_re_import_drift").await;
}

#[tokio::test]
async fn concurrent_double_import_yields_one_character_and_two_successes() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let body = crate::pbimport::fixtures::reference_export();
    // The race that matters: the SAME account importing twice at once (SC-5).
    seed_actor(&pool, "dev-sub-dave").await;
    let dave = player("dev-sub-dave");
    let (a, b) = tokio::join!(
        run_import(&pool, &dave, None, body),
        run_import(&pool, &dave, None, body)
    );
    let outcome_a = a.expect("first racer succeeds");
    let outcome_b = b.expect("second racer succeeds serially");
    assert_eq!(
        outcome_a.character.id, outcome_b.character.id,
        "both land on the same character"
    );
    let (rows,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM characters WHERE owner_sub = 'dev-sub-dave'")
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(rows, 1, "SC-5: never two characters for one account");
    testing::drop_test_db(pool, "store_concurrent").await;
}

#[tokio::test]
async fn six_accounts_hold_six_characters_in_one_party() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let subs = [
        "dev-sub-josh",
        "dev-sub-bear",
        "dev-sub-dave",
        "dev-sub-becky",
        "dev-sub-jake",
        "dev-sub-sixth",
    ];
    for sub in subs {
        seed_actor(&pool, sub).await;
    }
    let body = crate::pbimport::fixtures::reference_export();
    for sub in subs {
        let outcome = run_import(&pool, &player(sub), None, body)
            .await
            .expect("each seat imports");
        assert!(outcome.character.first_import);
    }
    let (characters,): (i64,) = sqlx::query_as("SELECT count(*) FROM characters")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(characters, 6, "SC-6: six accounts, six characters");
    let (parties,): (i64,) = sqlx::query_as("SELECT count(DISTINCT party_id) FROM characters")
        .fetch_one(&pool)
        .await
        .expect("parties");
    assert_eq!(parties, 1, "all in the single POC party");
    testing::drop_test_db(pool, "store_roster").await;
}

#[tokio::test]
async fn every_attempt_is_audited_with_outcome_and_target() {
    let Some(pool) = pool_or_skip().await else {
        return;
    };
    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    testing::seed_account(&pool, PLAYER_SUB, "player").await;
    testing::seed_account(&pool, "dev-sub-bear", "player").await;
    let josh = testing::seed_session(&pool, PLAYER_SUB, chrono::Utc::now()).await;
    let bear = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;

    // The allowed attempt: the reference export over the wire.
    let allowed_response = app
        .clone()
        .oneshot(post_import(
            &josh,
            crate::pbimport::fixtures::reference_export(),
        ))
        .await
        .unwrap();
    assert_eq!(allowed_response.status(), StatusCode::OK, "import succeeds");
    let character_id = response_json(allowed_response)
        .await
        .pointer("/character/id")
        .and_then(serde_json::Value::as_i64)
        .expect("character id");

    // The rejected attempt: class (a) body. Recording the denial belongs to
    // the handler that answers the 400 — asserted through that path, so the
    // test fails if the audit call goes missing.
    let denied_response = app.oneshot(post_import(&bear, "{not json")).await.unwrap();
    assert_eq!(denied_response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(denied_response).await.pointer("/error/code"),
        Some(&serde_json::json!("invalid-json")),
        "class (a) answers its envelope"
    );

    let rows: Vec<(String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT event, actor_sub, target, outcome FROM audit_events \
         WHERE event = 'character_import' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .expect("audit rows");
    assert_eq!(rows.len(), 2, "one audit row per attempt");
    let allowed = rows.first().expect("the allowed attempt is audited");
    let denied = rows.get(1).expect("the denied attempt is audited");
    assert_eq!(allowed.0, "character_import");
    assert_eq!(allowed.1.as_deref(), Some(PLAYER_SUB));
    assert_eq!(allowed.2, format!("character:{character_id}"));
    assert_eq!(allowed.3, "allowed");
    assert_eq!(denied.1.as_deref(), Some("dev-sub-bear"));
    assert_eq!(
        denied.2, "import:invalid-json",
        "failure target names the class"
    );
    assert_eq!(denied.3, "denied");
    testing::drop_test_db(pool, "store_audit").await;
}
