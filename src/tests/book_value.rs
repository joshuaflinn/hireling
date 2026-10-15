//! Integration tests for E12's book-value read (plan Task 5, spec FR-6,
//! design D5): one session-gated, party-scoped REST read resolving an item
//! name to the corpus's price — each through the real router (PR #30 rule).
//! The route reports prices and never converts: a found price echoes the
//! corpus coin object verbatim (pp included); the pp→gp conversion is the
//! sell apply's credit site, deliberately not this route's (D5).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use crate::auth::error::Forbidden;
use crate::auth::oidc;
use crate::testing;

/// A member's authenticated GET against the real router.
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

/// Seed one party member. Minimal on purpose: the party gate reads
/// `characters.party_id`/`owner_sub` and nothing else.
async fn seed_member(pool: &sqlx::PgPool, sub: &str) -> i64 {
    testing::seed_account(pool, sub, "player").await;
    let party_id: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('Stash') RETURNING id")
            .fetch_one(pool)
            .await
            .expect("party");
    sqlx::query(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, $2, '{}', '{}')",
    )
    .bind(party_id)
    .bind(sub)
    .execute(pool)
    .await
    .expect("member");
    party_id
}

/// Seed one corpus item row in the importer's stored shape —
/// `data.upstream` carries the foundry doc, so the price rides
/// `data->'upstream'->'system'->'price'->'value'` (import/transform.rs
/// `row_payload`). `price: Value::Null` seeds the missing-price-path shape.
async fn seed_item(pool: &sqlx::PgPool, name: &str, price: Value, lane: &str) -> i64 {
    let data = if lane == "custom" {
        // Custom rows (E9) carry a `custom` block and no upstream doc —
        // structurally unpriced.
        json!({"custom": {"description": "a one-off"}})
    } else {
        let system = if price.is_null() {
            json!({})
        } else {
            json!({"price": {"value": price}})
        };
        json!({"upstream": {"system": system}})
    };
    sqlx::query_scalar(
        "INSERT INTO corpus_entries (kind, name, lane, data) \
         VALUES ('item', $1, $2, $3) RETURNING id",
    )
    .bind(name)
    .bind(lane)
    .bind(data)
    .fetch_one(pool)
    .await
    .expect("corpus item row")
}

#[tokio::test]
async fn a_unique_match_reports_its_price() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    seed_item(&pool, "Longsword", json!({"gp": 15}), "imported").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Longsword"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("found"), Some(&Value::Bool(true)));
    assert_eq!(body.get("price"), Some(&json!({"gp": 15})));
    testing::drop_test_db(pool, "e12_bv_unique").await;
}

#[tokio::test]
async fn a_name_with_no_corpus_row_is_not_found() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    seed_item(&pool, "Longsword", json!({"gp": 15}), "imported").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Unobtainium"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"found": false}), "no price key when not found");
    testing::drop_test_db(pool, "e12_bv_no_match").await;
}

#[tokio::test]
async fn an_empty_price_value_is_no_book_value_but_a_zero_price_is_a_price() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    // The legacy OGL doc found in the probed pack: `price.value` present but
    // empty (foundry-packs contract §5). An empty coin object is unpriced.
    seed_item(&pool, "Ashberry", json!({}), "imported").await;
    // The other side of the same boundary: every coin at zero is a real,
    // reportable price, not an absence.
    seed_item(&pool, "Wooden Spoon", json!({"gp": 0}), "imported").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, ashberry) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Ashberry"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ashberry, json!({"found": false}), "empty coin object");
    let (spoon_status, spoon) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Wooden%20Spoon"),
        &session,
    )
    .await;
    assert_eq!(spoon_status, StatusCode::OK);
    assert_eq!(
        spoon,
        json!({"found": true, "price": {"gp": 0}}),
        "an all-zero price is a price"
    );
    testing::drop_test_db(pool, "e12_bv_empty_price").await;
}

#[tokio::test]
async fn a_missing_price_path_is_no_book_value() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    // An imported-shaped doc whose `system` carries no price...
    seed_item(&pool, "Ashberry", Value::Null, "imported").await;
    // ...and a custom row, which structurally has no upstream doc at all
    // (D5: custom rows read as unpriced — price presence decides).
    seed_item(&pool, "Vex's Compass", Value::Null, "custom").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    for name in ["Ashberry", "Vex%27s%20Compass"] {
        let (status, body) = get(
            &app,
            &format!("/api/parties/{party_id}/items/book-value?name={name}"),
            &session,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}");
        assert_eq!(body, json!({"found": false}), "{name}: missing price path");
    }
    testing::drop_test_db(pool, "e12_bv_missing_price").await;
}

#[tokio::test]
async fn two_rows_disagreeing_on_price_are_no_book_value() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    seed_item(&pool, "Longsword", json!({"gp": 15}), "imported").await;
    seed_item(&pool, "Longsword", json!({"gp": 1}), "imported").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Longsword"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"found": false}), "disagreeing prices refuse");
    testing::drop_test_db(pool, "e12_bv_disagree").await;
}

#[tokio::test]
async fn two_rows_agreeing_on_price_resolve_to_it() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    seed_item(&pool, "Steel Shield", json!({"gp": 20}), "imported").await;
    seed_item(&pool, "steel shield", json!({"gp": 20}), "imported").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Steel%20Shield"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({"found": true, "price": {"gp": 20}}),
        "agreeing prices collapse to the one value"
    );
    testing::drop_test_db(pool, "e12_bv_agree").await;
}

#[tokio::test]
async fn an_empty_priced_row_drops_out_of_a_disagreeing_set() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    // D5 reduces the match set to *distinct non-empty* coin objects: an
    // unpriced duplicate does not poison a priced one, and it cannot tip a
    // disagreement into an agreement.
    seed_item(&pool, "Longsword", json!({}), "imported").await;
    seed_item(&pool, "Longsword", json!({"gp": 15}), "imported").await;
    seed_item(&pool, "Battleaxe", json!({}), "imported").await;
    seed_item(&pool, "Battleaxe", json!({"gp": 10}), "imported").await;
    seed_item(&pool, "Battleaxe", json!({"gp": 4}), "imported").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, sword) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Longsword"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        sword,
        json!({"found": true, "price": {"gp": 15}}),
        "empty rows drop; the priced one stands"
    );
    let (axe_status, axe) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Battleaxe"),
        &session,
    )
    .await;
    assert_eq!(axe_status, StatusCode::OK);
    assert_eq!(
        axe,
        json!({"found": false}),
        "empty rows drop but cannot cure a disagreement"
    );
    testing::drop_test_db(pool, "e12_bv_empty_drops").await;
}

#[tokio::test]
async fn the_name_match_is_case_insensitive() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    seed_item(&pool, "Longsword", json!({"gp": 15}), "imported").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    for spelling in ["longsword", "LONGSWORD", "longSword"] {
        let (status, body) = get(
            &app,
            &format!("/api/parties/{party_id}/items/book-value?name={spelling}"),
            &session,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{spelling}");
        assert_eq!(
            body,
            json!({"found": true, "price": {"gp": 15}}),
            "{spelling}: the match is case-insensitive exact"
        );
    }
    testing::drop_test_db(pool, "e12_bv_case").await;
}

#[tokio::test]
async fn pp_rides_through_unconverted() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    seed_item(&pool, "Flask of Oil", json!({"pp": 1, "gp": 5}), "imported").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Flask%20of%20Oil"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({"found": true, "price": {"pp": 1, "gp": 5}}),
        "the route reports the corpus coin object verbatim — pp included; \
         the conversion is the sell apply's site, not this route's"
    );
    testing::drop_test_db(pool, "e12_bv_pp").await;
}

#[tokio::test]
async fn a_non_member_is_forbidden_with_the_e3_payload() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    seed_item(&pool, "Longsword", json!({"gp": 15}), "imported").await;
    testing::seed_account(&pool, "dev-sub-bear", "player").await;
    let outsider = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Longsword"),
        &outsider,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        serde_json::from_str::<Value>(Forbidden::body()).expect("e3 shape")
    );
    testing::drop_test_db(pool, "e12_bv_403").await;
}

#[tokio::test]
async fn the_gm_reads_it_too() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    seed_item(&pool, "Longsword", json!({"gp": 15}), "imported").await;
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let gm_session = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Longsword"),
        &gm_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GM reads succeed");
    assert_eq!(body, json!({"found": true, "price": {"gp": 15}}));
    testing::drop_test_db(pool, "e12_bv_gm").await;
}

#[tokio::test]
async fn a_missing_or_empty_name_is_a_bad_request() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let party_id = seed_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    for query in ["", "?name="] {
        let (status, body) = get(
            &app,
            &format!("/api/parties/{party_id}/items/book-value{query}"),
            &session,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "query {query:?}");
        assert_eq!(
            body.get("field"),
            Some(&Value::String("name".to_owned())),
            "the 400 names the field, custom.rs list shape"
        );
    }
    testing::drop_test_db(pool, "e12_bv_400").await;
}
