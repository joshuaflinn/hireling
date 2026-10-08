//! E10 roster read (plan Task 1): one party-scoped bootstrap read through
//! the real router. Reads only; member and GM both see the whole party;
//! outsiders never reach the handler (E3 middleware); the roster element
//! for a character is byte-equal to what `GET /api/characters/me` returns
//! for the same character (the shape contract, contracts/roster-rest.md).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt as _;

use super::test_router;
use crate::auth::oidc;
use crate::testing;

/// A member's authenticated GET against the real router (mirrors
/// `engine_rest.rs`'s helper verbatim — same session-cookie one-shot).
async fn get(app: &axum::Router, path: &str, session: &str) -> (StatusCode, Value) {
    let cookie = format!("{}={}", oidc::SESSION_COOKIE, session);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap_or_default();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

/// Seed a member into an explicit party — the general form. The imported-field
/// rule needs two fixtures differing in one field; the party-boundary test
/// needs a member in a party of their own. Returns (`party_id`, `character_id`).
async fn seed_member_in_party(
    pool: &sqlx::PgPool,
    party_id: i64,
    sub: &str,
    name: &str,
) -> (i64, i64) {
    const REFERENCE: &str = include_str!("../../tests/data/pb_export_reference.json");
    let export = crate::pbimport::model::parse_and_validate(REFERENCE).expect("valid reference");
    let (mut sheet, skips) = crate::pbimport::transform::transform(&export);
    assert!(
        skips.sections.is_empty(),
        "no skips in the reference export"
    );
    sheet.identity.name = name.to_owned();
    let sheet_json = serde_json::to_value(&sheet).expect("sheet serializes");
    testing::seed_account(pool, sub, "player").await;
    let character_id: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, $2, '{}', $3) RETURNING id",
    )
    .bind(party_id)
    .bind(sub)
    .bind(sheet_json)
    .fetch_one(pool)
    .await
    .expect("seed member");
    sqlx::query("INSERT INTO character_vitals (character_id) VALUES ($1)")
        .bind(character_id)
        .execute(pool)
        .await
        .expect("seed vitals");
    (party_id, character_id)
}

/// Seed a member into the single POC party (the default world). Returns
/// (`party_id`, `character_id`).
async fn seed_named_member(pool: &sqlx::PgPool, sub: &str, name: &str) -> (i64, i64) {
    let party_id: i64 = sqlx::query_scalar("SELECT id FROM parties ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("POC party");
    seed_member_in_party(pool, party_id, sub, name).await
}

#[tokio::test]
async fn the_roster_never_leaks_another_partys_characters() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (_party_id, josh_character) = seed_named_member(&pool, "dev-sub-josh", "Flinn").await;
    // A second party with a character of its own — the fixture the WHERE
    // clause answers (FR-5): scoped and unscoped queries must disagree
    // here, or the test proves nothing.
    let elsewhere: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('Elsewhere') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("seed second party");
    let (outsider_party, outsider_character) =
        seed_member_in_party(&pool, elsewhere, "dev-sub-outsider", "Outsider").await;
    assert_eq!(
        outsider_party, elsewhere,
        "the outsider lives in the second party"
    );
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(&app, "/api/party/roster", &session).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.get("party_id").and_then(Value::as_i64),
        Some(1),
        "josh resolves his own party, not the newest one"
    );
    let characters = body
        .get("characters")
        .and_then(Value::as_array)
        .expect("characters array");
    let ids: Vec<_> = characters
        .iter()
        .filter_map(|c| c.pointer("/character/id").and_then(Value::as_i64))
        .collect();
    assert_eq!(
        ids,
        vec![josh_character],
        "exactly josh's party — the other party's row never crosses"
    );
    assert_ne!(
        outsider_character, josh_character,
        "the fixture itself must not collapse the boundary away"
    );
    testing::drop_test_db(pool, "party_roster_scoped").await;
}

#[tokio::test]
async fn member_reads_the_whole_roster_through_the_router() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (_party_id, josh_character) = seed_named_member(&pool, "dev-sub-josh", "Flinn").await;
    let (_becky_party, becky_character) = seed_named_member(&pool, "dev-sub-becky", "Becky").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(&app, "/api/party/roster", &session).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.get("party_id").and_then(Value::as_i64),
        Some(1),
        "the seeded POC party"
    );
    assert_eq!(
        body.pointer("/you/sub").and_then(Value::as_str),
        Some("dev-sub-josh")
    );
    assert_eq!(
        body.pointer("/you/role").and_then(Value::as_str),
        Some("player")
    );
    let characters = body
        .get("characters")
        .and_then(Value::as_array)
        .expect("characters array");
    assert_eq!(
        characters.len(),
        2,
        "josh sees becky's character too — reads are ungated"
    );
    let josh_card = characters.first().expect("first roster card");
    let becky_card = characters.get(1).expect("second roster card");
    assert_eq!(
        josh_card.pointer("/character/id").and_then(Value::as_i64),
        Some(josh_character),
        "ORDER BY characters.id: the first-seeded member first"
    );
    assert_eq!(
        josh_card.pointer("/character/name").and_then(Value::as_str),
        Some("Flinn")
    );
    assert_eq!(
        josh_card
            .pointer("/character/owner")
            .and_then(Value::as_str),
        Some("dev-sub-josh")
    );
    assert_eq!(
        becky_card.pointer("/character/id").and_then(Value::as_i64),
        Some(becky_character)
    );
    assert_eq!(
        becky_card
            .pointer("/character/owner")
            .and_then(Value::as_str),
        Some("dev-sub-becky")
    );
    testing::drop_test_db(pool, "party_roster_member").await;
}

#[tokio::test]
async fn gm_reads_the_roster_without_owning_a_character() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    seed_named_member(&pool, "dev-sub-josh", "Flinn").await;
    // The GM seat: an account with the gm role and no character row —
    // resolution rides rule 2 (the single POC party).
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let gm_session = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(&app, "/api/party/roster", &gm_session).await;
    assert_eq!(status, StatusCode::OK, "the GM resolves the single party");
    assert_eq!(
        body.pointer("/you/role").and_then(Value::as_str),
        Some("gm")
    );
    assert_eq!(
        body.get("characters")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    testing::drop_test_db(pool, "party_roster_gm").await;
}

#[tokio::test]
async fn unauthenticated_get_is_401_before_the_handler() {
    let app = test_router();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/party/roster")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "require_auth sits in front of the roster read"
    );
}

#[tokio::test]
async fn roster_payload_equals_the_me_payload_for_the_same_character() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    seed_named_member(&pool, "dev-sub-josh", "Flinn").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (me_status, me_body) = get(&app, "/api/characters/me", &session).await;
    assert_eq!(me_status, StatusCode::OK);
    let (roster_status, roster_body) = get(&app, "/api/party/roster", &session).await;
    assert_eq!(roster_status, StatusCode::OK);
    let characters = roster_body
        .get("characters")
        .and_then(Value::as_array)
        .expect("characters array");
    assert_eq!(characters.len(), 1);
    assert_eq!(
        characters.first().expect("one roster card"),
        &me_body,
        "the roster element IS the me payload — field for field"
    );
    testing::drop_test_db(pool, "party_roster_shape").await;
}

#[tokio::test]
async fn two_imports_two_names_two_summaries() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    // Two fixtures differing ONLY in base_sheet.identity.name — the summary
    // must follow the sheet, never a constant.
    seed_named_member(&pool, "dev-sub-josh", "Flinn").await;
    seed_named_member(&pool, "dev-sub-becky", "Becky").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(&app, "/api/party/roster", &session).await;
    assert_eq!(status, StatusCode::OK);
    let characters = body
        .get("characters")
        .and_then(Value::as_array)
        .expect("characters array");
    let names: Vec<_> = characters
        .iter()
        .map(|c| {
            c.pointer("/character/name")
                .and_then(Value::as_str)
                .unwrap_or("")
        })
        .collect();
    assert_eq!(
        names,
        vec!["Flinn", "Becky"],
        "each summary carries its own sheet's name"
    );
    testing::drop_test_db(pool, "party_roster_names").await;
}

#[tokio::test]
async fn zero_parties_is_an_honest_409() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    seed_named_member(&pool, "dev-sub-josh", "Flinn").await;
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let gm_session = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;
    // Empty the world: characters cascade with their party; the GM owns
    // none anyway. Now the resolution rule has nothing to resolve.
    sqlx::query("DELETE FROM parties")
        .execute(&pool)
        .await
        .expect("clear parties");
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(&app, "/api/party/roster", &gm_session).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "zero parties is honest 409, never a guess"
    );
    assert_eq!(
        body,
        Value::Null,
        "the house error style: no body on the failure"
    );
    testing::drop_test_db(pool, "party_roster_409").await;
}

#[tokio::test]
async fn an_empty_party_is_200_with_an_empty_roster() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    seed_named_member(&pool, "dev-sub-josh", "Flinn").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    // Characters gone, the party stays: a characterless player in an
    // empty party is a valid, renderable state (data-model §1).
    sqlx::query("DELETE FROM characters")
        .execute(&pool)
        .await
        .expect("clear characters");
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(&app, "/api/party/roster", &session).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an empty roster is a state, not an error"
    );
    assert_eq!(body.get("party_id").and_then(Value::as_i64), Some(1));
    assert_eq!(
        body.get("characters")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0)
    );
    testing::drop_test_db(pool, "party_roster_empty").await;
}
