//! Integration tests for E9's custom rows REST (plan Tasks 5–6, spec
//! `specs/010-rules-tooltips-custom-content/contracts/custom-rows-rest.md`):
//! create + list + edit gate, all through the real router (PR #30 rule).
//! Caps are asserted on BOTH sides of every bound (AGENTS.md rules
//! exactness): 64/65, 280/281, 0/–1, 10/11, 20/21. Audit assertions read
//! the persisted `audit_events` rows, never a log line (observability rule).
//! FR-8 runs the real E4 corpus import pipeline over a minimal release
//! archive and asserts the custom rows byte-identical.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use crate::auth::error::Forbidden;
use crate::auth::oidc;
use crate::testing;

/// A real router over this test's isolated pool.
fn router(pool: &sqlx::PgPool) -> axum::Router {
    testing::router_for(pool.clone(), &testing::auth_settings())
}

/// An authenticated request with a JSON body against the real router.
async fn send_json(
    app: &axum::Router,
    method: &str,
    path: &str,
    session: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(session) = session {
        builder = builder.header("cookie", format!("{}={}", oidc::SESSION_COOKIE, session));
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap_or_default();
    let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

async fn get_json(app: &axum::Router, path: &str, session: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .header("cookie", format!("{}={}", oidc::SESSION_COOKIE, session))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap_or_default();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Seed a player account with one character in the POC party.
async fn seed_member(pool: &sqlx::PgPool, sub: &str) -> (i64, i64) {
    testing::seed_account(pool, sub, "player").await;
    let party_id: i64 = sqlx::query_scalar("SELECT id FROM parties ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("POC party seeded by migration 8");
    let character_id: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, $2, '{}', '{}') RETURNING id",
    )
    .bind(party_id)
    .bind(sub)
    .fetch_one(pool)
    .await
    .expect("seed member character");
    (party_id, character_id)
}

/// A second party for outsider tests (a second party is an INSERT — E2).
async fn seed_second_party(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar("INSERT INTO parties (name) VALUES ('Second Table') RETURNING id")
        .fetch_one(pool)
        .await
        .expect("seed second party")
}

/// The exact refusal payload every 403 carries (E3's byte-identical rule).
fn forbidden_body() -> Value {
    serde_json::from_str(Forbidden::body()).expect("the refusal payload is valid JSON")
}

/// Send, assert 201, and return the row body.
async fn assert_created(
    app: &axum::Router,
    method: &str,
    path: &str,
    session: &str,
    body: Value,
) -> Value {
    let (status, row) = send_json(app, method, path, Some(session), body).await;
    assert_eq!(status, StatusCode::CREATED, "expected 201: {row}");
    row
}

/// Send, and assert the contract's 400 shape names the field and the bound.
async fn assert_validation(
    app: &axum::Router,
    method: &str,
    path: &str,
    session: &str,
    body: Value,
    field: &str,
    reason: &str,
) {
    let (status, row) = send_json(app, method, path, Some(session), body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400: {row}");
    assert_eq!(
        row,
        json!({"error": "validation", "field": field, "reason": reason}),
        "the error names the field and the bound"
    );
}

/// One name of exactly `len` ASCII characters.
fn name_of_len(len: usize) -> String {
    "W".repeat(len)
}

// ---------------------------------------------------------------------------
// Task 5 — create + reads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_custom_condition_creates_and_surfaces_through_the_picker() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);

    let row = assert_created(
        &app,
        "POST",
        &format!("/api/parties/{party_id}/custom"),
        &session,
        json!({"kind": "condition", "name": "Sunlit", "description": "House reminder: standing in the sun."}),
    )
    .await;
    assert_eq!(row.get("lane").and_then(Value::as_str), Some("custom"));
    assert_eq!(row.get("kind").and_then(Value::as_str), Some("condition"));
    assert_eq!(row.get("name").and_then(Value::as_str), Some("Sunlit"));
    let corpus_entry_id = row
        .get("corpus_entry_id")
        .and_then(Value::as_i64)
        .expect("the created id rides the response");

    // The picker read (E8's route) shows the custom condition with the
    // NULL-tier default and no valued mapping — asserted, not assumed
    // (plan Task 5 done-when).
    let (status, picks) = get_json(
        &app,
        &format!("/api/parties/{party_id}/conditions"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pick = picks
        .as_array()
        .expect("a JSON array of picks")
        .iter()
        .find(|pick| pick.get("corpus_entry_id").and_then(Value::as_i64) == Some(corpus_entry_id))
        .expect("the custom condition joins the picker list");
    assert_eq!(pick.get("lane").and_then(Value::as_str), Some("custom"));
    assert_eq!(
        pick.get("tier").and_then(Value::as_str),
        Some("display_only"),
        "custom conditions are display-only by absence of an import block"
    );
    assert_eq!(
        pick.get("valued"),
        Some(&Value::Bool(false)),
        "custom conditions carry no modifier mappings"
    );
    testing::drop_test_db(pool, "custom_rows_condition_create").await;
}

#[tokio::test]
async fn a_custom_spell_creates_with_rank_and_surfaces_through_the_kind_read() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom");

    // Spell ranks are required — the first probe omits it on purpose.
    assert_validation(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "spell", "name": "Conjure Toad Swarm", "description": "500 toads."}),
        "value_or_rank",
        "required for spells",
    )
    .await;

    let row = assert_created(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "spell", "name": "Conjure Toad Swarm", "value_or_rank": 3, "description": "500 toads."}),
    )
    .await;
    assert_eq!(row.get("value_or_rank").and_then(Value::as_i64), Some(3));

    let (status, rows) = get_json(
        &app,
        &format!("/api/parties/{party_id}/custom?kind=spell"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let listed = rows
        .as_array()
        .expect("a JSON array of custom rows")
        .iter()
        .find(|listed_row| {
            listed_row.get("name").and_then(Value::as_str) == Some("Conjure Toad Swarm")
        })
        .expect("the custom spell lists for the composer");
    assert_eq!(listed.get("value_or_rank").and_then(Value::as_i64), Some(3));
    assert_eq!(
        listed.get("created_by_sub").and_then(Value::as_str),
        Some("dev-sub-josh"),
        "the creator rides the list row"
    );
    testing::drop_test_db(pool, "custom_rows_spell_create").await;
}

#[tokio::test]
async fn condition_kind_read_lists_custom_conditions_with_their_descriptions() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom");

    // Two rows, two different descriptions — the read must carry each row's
    // own description (US-1 AC-5's client-side chip join), not a shape
    // constant. A single-row literal cannot tell those apart.
    assert_created(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "condition", "name": "Sunlit", "description": "House reminder: standing in the sun."}),
    )
    .await;
    assert_created(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "condition", "name": "Winded", "description": "House reminder: catch your breath.", "value_or_rank": 2}),
    )
    .await;

    let (status, rows) = get_json(
        &app,
        &format!("/api/parties/{party_id}/custom?kind=condition"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let listed = rows.as_array().expect("a JSON array of custom rows");
    for (name, description) in [
        ("Sunlit", "House reminder: standing in the sun."),
        ("Winded", "House reminder: catch your breath."),
    ] {
        let row = listed
            .iter()
            .find(|row| row.get("name").and_then(Value::as_str) == Some(name))
            .unwrap_or_else(|| panic!("the custom condition {name} lists for the chip join"));
        assert_eq!(
            row.get("description").and_then(Value::as_str),
            Some(description),
            "the chip join reads this row's own description"
        );
        assert_eq!(
            row.get("created_by_sub").and_then(Value::as_str),
            Some("dev-sub-josh"),
            "the creator rides the list row"
        );
    }
    assert_eq!(
        listed
            .iter()
            .find(|row| row.get("name").and_then(Value::as_str) == Some("Winded"))
            .expect("the Winded row lists")
            .get("value_or_rank")
            .and_then(Value::as_i64),
        Some(2),
        "the optional display value rides the list row too"
    );

    // The kind bind filters: a spell never rides the condition read.
    assert_created(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "spell", "name": "Conjure Toad Swarm", "value_or_rank": 3, "description": "500 toads."}),
    )
    .await;
    let (_, relisted) = get_json(
        &app,
        &format!("/api/parties/{party_id}/custom?kind=condition"),
        &session,
    )
    .await;
    assert!(
        relisted
            .as_array()
            .expect("a JSON array")
            .iter()
            .all(|row| row.get("name").and_then(Value::as_str) != Some("Conjure Toad Swarm")),
        "spells stay out of the condition read"
    );
    testing::drop_test_db(pool, "custom_rows_condition_kind_read").await;
}

#[tokio::test]
async fn kind_read_refusals_name_every_accepted_kind() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);

    // Missing kind and unknown kind both name every kind the read accepts.
    for query in ["", "?kind=feat"] {
        assert_validation(
            &app,
            "GET",
            &format!("/api/parties/{party_id}/custom{query}"),
            &session,
            json!({}),
            "kind",
            "spell, item, or condition",
        )
        .await;
    }
    testing::drop_test_db(pool, "custom_rows_kind_read_refusals").await;
}

#[tokio::test]
async fn a_second_create_of_the_same_custom_item_accumulates_the_inventory_row() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom");
    let body = json!({"kind": "item", "name": "Lucky Rock", "description": "It is a rock."});

    // Contract §3: duplicate names are allowed. The second create of the
    // same custom item must answer 201 and accumulate the inventory anchor
    // — not 500 on the (character_id, item_name) primary key.
    assert_created(&app, "POST", &path, &session, body.clone()).await;
    assert_created(&app, "POST", &path, &session, body).await;

    let (_, rows) = get_json(
        &app,
        &format!("/api/parties/{party_id}/custom?kind=item"),
        &session,
    )
    .await;
    let lucky = rows
        .as_array()
        .expect("a JSON array of custom rows")
        .iter()
        .filter(|row| row.get("name").and_then(Value::as_str) == Some("Lucky Rock"))
        .count();
    assert_eq!(lucky, 2, "duplicate corpus names stay distinct rows (§3)");

    let qty_delta: i32 = sqlx::query_scalar(
        "SELECT qty_delta FROM character_inventory_live \
         WHERE character_id = $1 AND item_name = 'Lucky Rock'",
    )
    .bind(character_id)
    .fetch_one(&pool)
    .await
    .expect("one inventory anchor row for the name");
    assert_eq!(
        qty_delta, 2,
        "the second create accumulates the anchor: the character holds two"
    );
    testing::drop_test_db(pool, "custom_rows_item_name_collision").await;
}

#[tokio::test]
async fn an_edit_through_a_foreign_or_nonexistent_party_is_404() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let other_party = seed_second_party(&pool).await;
    let row_id = seed_custom_row(&pool, "dev-sub-josh", "Sunlit").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);

    // The path's party segment must name a real party the actor belongs
    // to: a foreign party and a nonexistent one are both resource paths
    // that do not exist — 404, not a successful edit of the very same row.
    for wrong in [other_party, 999_999] {
        let (status, body) = send_json(
            &app,
            "PATCH",
            &format!("/api/parties/{wrong}/custom/{row_id}"),
            Some(&session),
            json!({"description": "Renamed through a party that is not mine."}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "party {wrong}: {body}");
    }

    // Control: the creator's own party still edits normally.
    let (status, _) = send_json(
        &app,
        "PATCH",
        &format!("/api/parties/{party_id}/custom/{row_id}"),
        Some(&session),
        json!({"description": "Revised in the row's own party."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    testing::drop_test_db(pool, "custom_rows_edit_party_segment").await;
}

#[tokio::test]
async fn a_custom_item_writes_the_inventory_row_with_qty_delta_one() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, character_id) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);

    assert_created(
        &app,
        "POST",
        &format!("/api/parties/{party_id}/custom"),
        &session,
        json!({"kind": "item", "name": "Looted Wagon", "description": "Mostly bandits' junk."}),
    )
    .await;

    // US-3 AC-1: quantity renders base_qty + delta; a custom item has no
    // anchor base, so the delta MUST be the explicit 1 — the column
    // default 0 would render qty 0 (contract §1, data-model §1).
    let (stored_qty, stored_character, stored_name): (i32, i64, String) =
        sqlx::query_as("SELECT qty_delta, character_id, item_name FROM character_inventory_live")
            .fetch_one(&pool)
            .await
            .expect("the inventory row exists");
    assert_eq!(
        stored_qty, 1,
        "qty_delta is the explicit 1, never the default 0"
    );
    assert_eq!(
        stored_character, character_id,
        "the creator's character owns the row"
    );
    assert_eq!(stored_name, "Looted Wagon");

    let (status, rows) = get_json(
        &app,
        &format!("/api/parties/{party_id}/custom?kind=item"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        rows.as_array()
            .expect("array")
            .iter()
            .any(|row| row.get("name").and_then(Value::as_str) == Some("Looted Wagon")),
        "the custom item lists for the inventory merge: {rows}"
    );
    testing::drop_test_db(pool, "custom_rows_item_create").await;
}

#[tokio::test]
async fn item_creates_reject_value_or_rank() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);

    assert_validation(
        &app,
        "POST",
        &format!("/api/parties/{party_id}/custom"),
        &session,
        json!({"kind": "item", "name": "Looted Wagon", "value_or_rank": 1}),
        "value_or_rank",
        "not allowed for items",
    )
    .await;
    testing::drop_test_db(pool, "custom_rows_item_value").await;
}

#[tokio::test]
async fn name_caps_hold_on_both_sides_of_the_bound() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom");

    // At the bound (64): admitted.
    let at_cap = assert_created(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "item", "name": name_of_len(64)}),
    )
    .await;
    assert_eq!(
        at_cap.get("name").and_then(Value::as_str),
        Some(name_of_len(64).as_str())
    );

    // Over the bound (65): refused, field named.
    assert_validation(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "item", "name": name_of_len(65)}),
        "name",
        "1..64 characters after trim",
    )
    .await;

    // Under the bound (empty and whitespace-only): refused.
    for blank in ["", "   "] {
        assert_validation(
            &app,
            "POST",
            &path,
            &session,
            json!({"kind": "item", "name": blank}),
            "name",
            "1..64 characters after trim",
        )
        .await;
    }

    // Input is trimmed before the bound and before the write.
    let trimmed = assert_created(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "item", "name": "  Padded Wagon  "}),
    )
    .await;
    assert_eq!(
        trimmed.get("name").and_then(Value::as_str),
        Some("Padded Wagon"),
        "the stored name is the trimmed one"
    );
    testing::drop_test_db(pool, "custom_rows_name_caps").await;
}

#[tokio::test]
async fn description_caps_hold_on_both_sides_of_the_bound() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom");

    let at_cap = assert_created(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "condition", "name": "Padded", "description": "x".repeat(280)}),
    )
    .await;
    assert_eq!(
        at_cap.get("description").and_then(Value::as_str),
        Some("x".repeat(280).as_str()),
        "280 characters fit and store verbatim"
    );

    assert_validation(
        &app,
        "POST",
        &path,
        &session,
        json!({"kind": "condition", "name": "Overlong", "description": "x".repeat(281)}),
        "description",
        "0..280 characters after trim",
    )
    .await;
    testing::drop_test_db(pool, "custom_rows_description_caps").await;
}

#[tokio::test]
async fn spell_rank_caps_hold_on_both_sides_of_the_bound() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom");

    for rank in [0, 10] {
        assert_created(
            &app,
            "POST",
            &path,
            &session,
            json!({"kind": "spell", "name": format!("Rank {rank} Toads"), "value_or_rank": rank}),
        )
        .await;
    }
    for rank in [11, -1] {
        assert_validation(
            &app,
            "POST",
            &path,
            &session,
            json!({"kind": "spell", "name": format!("Rank {rank} Toads"), "value_or_rank": rank}),
            "value_or_rank",
            "0..10 for spells",
        )
        .await;
    }
    testing::drop_test_db(pool, "custom_rows_spell_rank_caps").await;
}

#[tokio::test]
async fn condition_value_caps_hold_on_both_sides_of_the_bound() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom");

    for value in [1, 20] {
        assert_created(
            &app,
            "POST",
            &path,
            &session,
            json!({"kind": "condition", "name": format!("Valued {value}"), "value_or_rank": value}),
        )
        .await;
    }
    for value in [0, 21] {
        assert_validation(
            &app,
            "POST",
            &path,
            &session,
            json!({"kind": "condition", "name": format!("Valued {value}"), "value_or_rank": value}),
            "value_or_rank",
            "1..20 for conditions",
        )
        .await;
    }
    testing::drop_test_db(pool, "custom_rows_condition_value_caps").await;
}

#[tokio::test]
async fn unknown_kinds_refuse_with_the_field_named() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);

    for kind in ["feat", ""] {
        assert_validation(
            &app,
            "POST",
            &format!("/api/parties/{party_id}/custom"),
            &session,
            json!({"kind": kind, "name": "Whatever"}),
            "kind",
            "item, spell, or condition",
        )
        .await;
    }
    testing::drop_test_db(pool, "custom_rows_kind_caps").await;
}

#[tokio::test]
async fn a_member_of_another_party_cannot_create_into_this_one() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let outsider_party = seed_second_party(&pool).await;
    testing::seed_account(&pool, "dev-sub-bear", "player").await;
    sqlx::query(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, 'dev-sub-bear', '{}', '{}')",
    )
    .bind(outsider_party)
    .execute(&pool)
    .await
    .expect("seed outsider character");
    let session = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let app = router(&pool);

    let (status, row) = send_json(
        &app,
        "POST",
        &format!("/api/parties/{party_id}/custom"),
        Some(&session),
        json!({"kind": "item", "name": "Smuggled In"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "membership gates writes");
    assert_eq!(row, forbidden_body());
    testing::drop_test_db(pool, "custom_rows_outsider_create").await;
}

#[tokio::test]
async fn the_gm_cannot_create_the_read_only_seat_holds() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    testing::seed_account(&pool, testing::GM_SUB, "gm").await;
    let session = testing::seed_session(&pool, testing::GM_SUB, chrono::Utc::now()).await;
    let app = router(&pool);

    let (status, row) = send_json(
        &app,
        "POST",
        &format!("/api/parties/{party_id}/custom"),
        Some(&session),
        json!({"kind": "item", "name": "GM Fiat Wagon"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the GM writes nothing (E3 FR-12)"
    );
    assert_eq!(row, forbidden_body());
    // The denial is audited on the production path that refuses it: the
    // gm_read_only middleware (E3) intercepts every mutating method before
    // any handler, so the persisted event is `forbidden_gm_write` — the
    // contract's PATCH-path event name applies to denials that reach the
    // handler; a GM denial structurally cannot.
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits.iter().any(|(event, actor, target, outcome)| {
            event == "forbidden_gm_write"
                && actor.as_deref() == Some(testing::GM_SUB)
                && target.contains("/custom")
                && outcome == "denied"
        }),
        "the GM denial is audited: {audits:?}"
    );
    testing::drop_test_db(pool, "custom_rows_gm_create").await;
}

// ---------------------------------------------------------------------------
// Task 6 — edit gate + audit + importer isolation
// ---------------------------------------------------------------------------

/// Seed one custom condition row directly (no HTTP) and return its id.
async fn seed_custom_row(pool: &sqlx::PgPool, sub: &str, name: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO corpus_entries (kind, name, lane, data, created_by_sub) \
         VALUES ('condition', $2, 'custom', $3, $1) RETURNING id",
    )
    .bind(sub)
    .bind(name)
    .bind(json!({"custom": {"description": "seeded"}}))
    .fetch_one(pool)
    .await
    .expect("seed custom row")
}

#[tokio::test]
async fn the_creator_edits_their_own_row() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let row_id = seed_custom_row(&pool, "dev-sub-josh", "Sunlit").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom/{row_id}");

    let (status, row) = send_json(
        &app,
        "PATCH",
        &path,
        Some(&session),
        json!({"description": "Revised: standing in the noon sun.", "value_or_rank": 2}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the creator is the sole writer: {row}"
    );
    assert_eq!(
        row.get("description").and_then(Value::as_str),
        Some("Revised: standing in the noon sun.")
    );
    assert_eq!(row.get("value_or_rank").and_then(Value::as_i64), Some(2));

    // Successful edits are NOT audit events (data-model §2).
    let audits = testing::audit_rows(&pool).await;
    assert!(
        !audits
            .iter()
            .any(|(event, _, _, _)| event == "forbidden_custom_write"),
        "allowed edits audit nothing: {audits:?}"
    );
    testing::drop_test_db(pool, "custom_rows_creator_edit").await;
}

#[tokio::test]
async fn another_members_edit_is_403_and_audited() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let row_id = seed_custom_row(&pool, "dev-sub-josh", "Sunlit").await;
    seed_member(&pool, "dev-sub-bear").await;
    let session = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let app = router(&pool);

    let (status, row) = send_json(
        &app,
        "PATCH",
        &format!("/api/parties/{party_id}/custom/{row_id}"),
        Some(&session),
        json!({"name": "Hijacked"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(row, forbidden_body());

    // The persisted audit row is the assertion target (observability rule):
    // event, actor, target, outcome — read from the table.
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits.iter().any(|(event, actor, target, outcome)| {
            event == "forbidden_custom_write"
                && actor.as_deref() == Some("dev-sub-bear")
                && target.as_str() == format!("custom/{row_id}")
                && outcome == "denied"
        }),
        "the forbidden edit persists its audit row: {audits:?}"
    );
    testing::drop_test_db(pool, "custom_rows_other_member_edit").await;
}

#[tokio::test]
async fn the_gms_edit_is_403_and_audited() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let row_id = seed_custom_row(&pool, "dev-sub-josh", "Sunlit").await;
    testing::seed_account(&pool, testing::GM_SUB, "gm").await;
    let session = testing::seed_session(&pool, testing::GM_SUB, chrono::Utc::now()).await;
    let app = router(&pool);

    let (status, row) = send_json(
        &app,
        "PATCH",
        &format!("/api/parties/{party_id}/custom/{row_id}"),
        Some(&session),
        json!({"description": "GM override"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(row, forbidden_body());
    // See the create-side note: the middleware refuses GM writes before the
    // handler and persists `forbidden_gm_write` — the production path owns
    // the event name.
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits.iter().any(|(event, actor, _, outcome)| {
            event == "forbidden_gm_write"
                && actor.as_deref() == Some(testing::GM_SUB)
                && outcome == "denied"
        }),
        "the GM edit denial is audited: {audits:?}"
    );
    testing::drop_test_db(pool, "custom_rows_gm_edit").await;
}

#[tokio::test]
async fn editing_a_non_custom_lane_row_is_403_and_audited() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let imported_id: i64 = sqlx::query_scalar(
        "INSERT INTO corpus_entries (kind, name, lane, data, source_id, created_by_sub) \
         VALUES ('condition', 'Frightened', 'imported', '{}', 'up-1', 'dev-sub-josh') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("seed imported row");
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);

    let (status, row) = send_json(
        &app,
        "PATCH",
        &format!("/api/parties/{party_id}/custom/{imported_id}"),
        Some(&session),
        json!({"name": "Renamed From The Side Door"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "curation editing is E15");
    assert_eq!(row, forbidden_body());
    let audits = testing::audit_rows(&pool).await;
    assert!(
        audits.iter().any(|(event, _, target, outcome)| {
            event == "forbidden_custom_write"
                && target.as_str() == format!("custom/{imported_id}")
                && outcome == "denied"
        }),
        "the lane violation persists its audit row: {audits:?}"
    );
    testing::drop_test_db(pool, "custom_rows_imported_edit").await;
}

#[tokio::test]
async fn patch_caps_and_patchable_fields_hold() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let row_id = seed_custom_row(&pool, "dev-sub-josh", "Sunlit").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);
    let path = format!("/api/parties/{party_id}/custom/{row_id}");

    // Over-cap name through the edit path: refused.
    assert_validation(
        &app,
        "PATCH",
        &path,
        &session,
        json!({"name": name_of_len(65)}),
        "name",
        "1..64 characters after trim",
    )
    .await;

    // `kind` is not patchable — the lane a row lives in never changes.
    assert_validation(
        &app,
        "PATCH",
        &path,
        &session,
        json!({"kind": "spell"}),
        "kind",
        "unknown field",
    )
    .await;

    // An unknown row is a plain 404.
    let (ghost_status, _) = send_json(
        &app,
        "PATCH",
        &format!("/api/parties/{party_id}/custom/999999"),
        Some(&session),
        json!({"name": "Ghost"}),
    )
    .await;
    assert_eq!(ghost_status, StatusCode::NOT_FOUND);
    testing::drop_test_db(pool, "custom_rows_patch_caps").await;
}

// ---------------------------------------------------------------------------
// FR-8 — the corpus importer's re-runs never touch custom rows
// ---------------------------------------------------------------------------

/// A minimal release archive: one condition document and one equipment
/// document, each valid per the importer's declared shape
/// (`src/import/model.rs::parse_doc`). The condition reuses the seed's
/// frightened id so the run maps cleanly.
fn minimal_pack_zip() -> Vec<u8> {
    use std::io::Write as _;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    let condition = json!([{
        "_id": "TBSHQspnbcqxsmjL",
        "name": "Frightened",
        "type": "condition",
        "img": "systems/pf2e/icons/conditions/frightened.webp",
        "system": {
            "description": {"value": "Status penalty equal to the value."},
            "publication": {"license": "ORC", "title": "Pathfinder Player Core", "remaster": true},
            "value": {"isValued": true}
        }
    }]);
    let equipment = json!([{
        "_id": "up-eq-1",
        "name": "Test Rations",
        "type": "equipment",
        "img": "systems/pf2e/icons/equipment/rations.webp",
        "system": {
            "description": {"value": "A week of trail food."},
            "publication": {"license": "ORC", "title": "Pathfinder Player Core", "remaster": true}
        }
    }]);
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buffer);
        let options = SimpleFileOptions::default();
        for (name, docs) in [
            ("packs/conditions.json", &condition),
            ("packs/equipment.json", &equipment),
        ] {
            zip.start_file(name, options).expect("zip entry starts");
            zip.write_all(docs.to_string().as_bytes())
                .expect("zip entry writes");
        }
        zip.finish().expect("zip finishes");
    }
    buffer.into_inner()
}

/// The full row of every custom-lane corpus row, as Postgres sees it.
async fn custom_rows_snapshot(
    pool: &sqlx::PgPool,
) -> Vec<(i64, String, String, Value, Option<String>)> {
    sqlx::query_as(
        "SELECT id, kind, name, data, created_by_sub FROM corpus_entries \
         WHERE lane = 'custom' ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .expect("custom rows read back")
}

#[tokio::test]
async fn fr8_corpus_import_reruns_leave_custom_rows_byte_identical() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = router(&pool);

    // One of each custom kind, through the real create path — including the
    // inventory row a custom item carries.
    for body in [
        json!({"kind": "condition", "name": "Sunlit", "description": "Standing in the sun."}),
        json!({"kind": "spell", "name": "Conjure Toad Swarm", "value_or_rank": 3}),
        json!({"kind": "item", "name": "Looted Wagon"}),
    ] {
        assert_created(
            &app,
            "POST",
            &format!("/api/parties/{party_id}/custom"),
            &session,
            body,
        )
        .await;
    }
    let before = custom_rows_snapshot(&pool).await;
    assert_eq!(before.len(), 3, "the three custom rows exist: {before:?}");
    let inventory_before: i64 = sqlx::query_scalar("SELECT count(*) FROM character_inventory_live")
        .fetch_one(&pool)
        .await
        .expect("inventory count");

    // The REAL E4 pipeline (import_from_bytes — run_import minus the
    // network fetch), against a minimal but valid release archive.
    let seed_json = include_str!("../../data/seed/condition-tiers.json");
    let report =
        crate::import::import_from_bytes(&pool, "pf2e-8.5.1", &minimal_pack_zip(), seed_json)
            .await
            .expect("the import run succeeds");
    assert!(report.success, "the run reports success");

    // The import actually imported — the re-run is real, not a silent no-op.
    let imported: i64 =
        sqlx::query_scalar("SELECT count(*) FROM corpus_entries WHERE lane = 'imported'")
            .fetch_one(&pool)
            .await
            .expect("imported count");
    assert_eq!(imported, 2, "both pack documents landed: {imported}");

    // FR-8: the custom rows are byte-identical before and after.
    let after = custom_rows_snapshot(&pool).await;
    assert_eq!(before, after, "custom rows survive the re-run untouched");
    let inventory_after: i64 = sqlx::query_scalar("SELECT count(*) FROM character_inventory_live")
        .fetch_one(&pool)
        .await
        .expect("inventory count");
    assert_eq!(
        inventory_before, inventory_after,
        "the inventory row is untouched too"
    );
    testing::drop_test_db(pool, "custom_rows_fr8_import_isolation").await;
}
