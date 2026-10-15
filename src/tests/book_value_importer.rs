//! The joint between the two halves of the read (E12 plan Task 5, spec FR-6,
//! design D5) — round 2 of review rejected the route's first tests for
//! proving only themselves.
//!
//! `tests/book_value.rs` seeds corpus rows in the shape it *believes* the
//! importer writes, then asserts the route reads them back. That suite cannot
//! fail when the belief is wrong: it builds the row and reads the path with
//! the same hand. Which is how it stayed green against the D5-SQL defect that
//! answers "no book value" for every priced row in a real hall. What is
//! proven here is the joint — the importer's write path and the route's read
//! path, with nothing of this suite's shape in between.
//!
//! Both tests drive the route through the configured router, never the module
//! underneath it (AGENTS.md: a spec'd behavior is proven on the production
//! path that owns it). `resolve_lookup` stays private to `book_value` and
//! that is load-bearing: with the seam private, a session-less call into the
//! lookup does not compile, so the HTTP surface is the only way in.
//!
//! The expected price is read out of the captured fixture, never restated —
//! so no literal is shared across the joint, and the wrapper convention is
//! asserted without naming it twice.
//!
//! Three mutations were run against this file and `book_value.rs` to prove the
//! two halves are not decorative. All three are caught; all three were reverted.
//!
//! - **M1 — producer moves.** Rename `row_payload`'s `upstream` wrapper key.
//!   Both halves fail: the row no longer sits under the path the read is
//!   written against.
//! - **M2 — consumer moves.** Restore the path design D5 sketched, leaving the
//!   producer correct. Both halves fail, and the second one is the one that
//!   matters: `{found: false}` for a row that is priced and present. This is
//!   the bug round 1 rejected the branch for, and it is the shape the
//!   hand-seeded fixtures could not see — they built the row and read the path
//!   with the same hand, so a wrong path was wrong in both.
//! - **M2b — consumer moves at the leaf.** Rename the read's final key
//!   (`price.value` → `price.coins`). Both halves fail.
//!
//! One mutation is *not* caught, and naming it is the honest version of this
//! file's coverage claim: **M3 — drop `kind = 'item'` from the read.** Both
//! halves stay green, because the captured import seeds no corpus row that
//! shares a name with an item. The `kind` filter is a correctness preference
//! carried by `tests/book_value.rs`'s fixtures and by review of the SQL, not by
//! this joint; a same-named condition row is the fixture that would carry it,
//! and it is not here. Say so rather than let a green suite imply it.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use crate::auth::oidc;
use crate::import::import_from_bytes;
use crate::testing;

/// The captured foundry document for the Wayfinder, verbatim off disk. The
/// importer's input, and the only place this file's expectations come from.
///
/// # Panics
///
/// Panics if the fixture is missing or unreadable — a moved fixture, not a
/// swallowed error.
fn captured_doc() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/packs/wayfinder.json"
    ))
    .expect("captured item fixture");
    serde_json::from_str(&text).expect("captured item fixture is JSON")
}

/// The price the route is supposed to find, walked out of the captured
/// document rather than written here. Expected values come from the file on
/// disk, so no literal in this file is shared with the importer's output.
///
/// # Panics
///
/// Panics if the fixture has no `system.price.value` — the fixture moving out
/// from under the first test, which then says so.
fn captured_price() -> Value {
    let doc = captured_doc();
    let system = doc.get("system").expect("fixture carries a system block");
    let price = system.get("price").expect("fixture carries a price");
    price
        .get("value")
        .expect("fixture carries a price value")
        .clone()
}

/// A DB with the real importer run over the captured document, through the
/// same `import_from_bytes` the CLI write path runs (`run_import` minus the
/// network fetch) — a fixture that is not a fixture. The archive is composed
/// with the same `testing::pack_zip` the E4/E5 suites use; `pack.rs`'s own
/// tests cover the pack-file reading side of that seam. A conditions pack
/// rides along because the importer refuses a zero-document pack of either
/// kind (FR-12); that helper's doc states the reason, and none of its rows
/// become items.
///
/// Returns `None` when Postgres is unavailable, per the suite convention.
///
/// # Panics
///
/// Panics if the captured document is not the item the fixture says it is, or
/// the import run fails.
async fn imported_pool() -> Option<sqlx::PgPool> {
    let pool = testing::test_pool().await?;
    let doc = captured_doc();
    let name = doc.get("name").expect("fixture carries a name");
    let kind = doc.get("type").expect("fixture carries a type");
    assert_eq!(name.as_str().expect("name"), "Wayfinder");
    assert_eq!(kind.as_str().expect("type"), "equipment");
    let zip = testing::pack_zip(&[
        (
            "packs/conditions.json",
            json!([{"_id": "TBSHQspnbcqxsmjL",
            "name": "Frightened",
            "type": "condition",
            "img": "systems/pf2e/icons/conditions/frightened.webp",
            "system": {
                "description": {"value": "Status penalty equal to the value."},
                "publication": {"license": "ORC", "title": "Pathfinder Player Core", "remaster": true},
                "value": {"isValued": true}
            }}]),
        ),
        ("packs/equipment.json", json!([doc])),
    ]);
    // The document above is the *input*; the wrapper the route's stored path
    // is written against is added by `import::transform::row_payload`, so
    // renaming that key breaks the assertions below, not this call.
    let report = import_from_bytes(
        &pool,
        "pf2e-8.5.1",
        &zip,
        include_str!("../../data/seed/condition-tiers.json"),
    )
    .await
    .expect("the captured document imports");
    assert!(
        report.success,
        "the import run succeeds: {:?}",
        report.error
    );
    Some(pool)
}

/// An authenticated GET against the real router. Local to this file on
/// purpose: the route's HTTP surface *is* the assertion here, so a shared
/// helper would be one more thing this test could be read as bypassing.
///
/// # Panics
///
/// Panics on a malformed request or unreadable body.
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Seed an account, a party and a session, minimally. The authz half of the
/// route is `tests/book_value.rs`'s business; here the party row exists so the
/// readability gate lets the request reach the lookup.
///
/// # Panics
///
/// Panics on a failed seed insert.
async fn seeded_party(pool: &sqlx::PgPool, sub: &str) -> i64 {
    testing::seed_account(pool, sub, "player").await;
    let party_id: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('Book value') RETURNING id")
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

/// Half one of the joint: the row the importer actually wrote carries the
/// captured price at the path E12 reads. Producer side is the real write
/// path, consumer side is the route's literal SQL path copied from
/// `resolve_lookup`'s statement — rename the importer's `upstream` wrapper
/// key and this fails.
#[tokio::test]
async fn the_importer_stores_the_price_where_the_route_reads_it() {
    let Some(pool) = imported_pool().await else {
        return;
    };
    let stored: Option<Value> = sqlx::query_scalar(
        "SELECT data->'upstream'->'system'->'price'->'value' \
         FROM corpus_entries WHERE kind = 'item' AND name = 'Wayfinder'",
    )
    .fetch_one(&pool)
    .await
    .expect("the imported row exists");
    assert_eq!(
        stored.as_ref(),
        Some(&captured_price()),
        "E12's book-value read is a stored-path read of \
         data->'upstream'->'system'->'price'->'value' (the path in \
         `book_value::resolve_lookup`'s SQL); the importer must leave the \
         captured wayfinder price sitting at it"
    );
    testing::drop_test_db(pool, "e12_bv_importer_path").await;
}

/// Half two, on the route's own surface: import the captured document through
/// the real pipeline, then ask `GET /api/parties/{party}/items/book-value` for
/// that item and read the answer. A row `store::apply_category` wrote, not one
/// this suite built, resolves to its book value. Rename the importer's wrapper
/// key, or restore the path design D5 sketched, and this fails.
#[tokio::test]
async fn the_route_answers_for_an_item_the_importer_wrote() {
    let Some(pool) = imported_pool().await else {
        return;
    };
    let party_id = seeded_party(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/items/book-value?name=Wayfinder"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.get("found"),
        Some(&Value::Bool(true)),
        "the importer's own row must resolve to its book value. `found: false` \
         is the defect round 1 found: the read path and the stored shape disagree, \
         and the route answers 'no book value' for every priced row"
    );
    assert_eq!(body.get("price"), Some(&captured_price()));
    testing::drop_test_db(pool, "e12_bv_importer_route").await;
}
