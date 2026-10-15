//! The book-value route's stored-path joint, closed against the REAL
//! importer (round-1 review finding, PR #227).
//!
//! `src/tests/book_value.rs` seeds corpus rows through `seed_item`, which
//! hand-builds `json!({"upstream": {"system": …}})` — the same literal path
//! the route's SQL reads. Both sides were written from one reading of
//! `import::transform::row_payload`, so that suite cannot disagree with the
//! query: it proves the reduction and the authz, not the path. This file
//! drives the route's lookup half against a row the importer itself planned
//! and wrote — `import_from_bytes` over a release archive built from the
//! captured `tests/fixtures/packs/wayfinder.json` — so the fixture's shape is the
//! importer's output, not this suite's belief about it. Rename
//! `row_payload`'s item-branch wrapper key and both tests below go red —
//! one because the capture no longer lands where the route looks, one because
//! the route stops finding a price the pack says is there (mutation-checked:
//! the failure scenario is a one-token edit to `transform.rs::row_payload`,
//! not an argument. The condition branch's identical wrapper is not covered
//! here and is not claimed: this route reads only item rows, and its shape is
//! the importer suite's to guard).
//!
//! Split out from `book_value.rs` rather than appended to it: that file's
//! discipline is the HTTP surface through the router, and the half proven
//! here deliberately reaches the lookup without a session — see
//! `book_value::lookup`.

use axum::http::StatusCode;
use axum::response::Response;
use serde_json::{Value, json};

use crate::import::import_from_bytes;
use crate::testing;

/// The captured fixture's price, read from the fixture file rather than
/// retyped. The assertion's other half is the route's literal SQL path, so
/// the two sides share no hand-written literal.
///
/// # Panics
///
/// Panics if the checked-in fixture lacks the price path — that absence is
/// what these tests are guarding, so it fails here loudly.
fn captured_price() -> Value {
    fixture("wayfinder.json")
        .pointer("/system/price/value")
        .cloned()
        .expect("the captured wayfinder carries system.price.value")
}

/// Read one checked-in fixture as JSON.
///
/// # Panics
///
/// Panics if the fixture is unreadable or is not JSON — a broken fixture is a
/// failing test.
fn fixture(name: &str) -> Value {
    repo_file(&format!("tests/fixtures/packs/{name}"))
}

/// Read a checked-in repo file as JSON, relative to the crate root.
///
/// # Panics
///
/// Panics if the file is unreadable or is not JSON.
fn repo_file(relative: &str) -> Value {
    let path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), relative);
    let raw = std::fs::read_to_string(&path).expect("repo file readable");
    serde_json::from_str(&raw).expect("repo file is JSON")
}

/// The checked-in tier seed, as the importer's own `SEED_JSON` constant holds
/// it (that constant is private; the file is the same source).
///
/// # Panics
///
/// Panics if the seed file is unreadable.
fn tier_seed() -> String {
    std::fs::read_to_string(format!(
        "{}/data/seed/condition-tiers.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("tier seed readable")
}

/// The captured conditions pack for the run: the importer refuses a
/// zero-document pack of either kind (FR-12), so an item-only import still has
/// to ship one. Both captured condition fixtures ride along, the way
/// `tests/import_db.rs`'s release fixtures do.
fn captured_conditions_pack() -> Value {
    Value::Array(vec![fixture("frightened.json"), fixture("concealed.json")])
}

/// The captured wayfinder document, verbatim (no mutation, no re-shaping),
/// as the item pack body the importer reads.
///
/// # Panics
///
/// Panics if the checked-in fixture is unreadable or is not JSON.
fn captured_item_pack() -> Value {
    Value::Array(vec![fixture("wayfinder.json")])
}

/// Run the real import pipeline (`import_from_bytes`) over the captured
/// wayfinder: the whole store-side chain — extraction, `row_payload`,
/// `plan_category`, `store::apply_category` — with no network leg. Nothing
/// here hand-builds a corpus row: the `data` jsonb under test is
/// `row_payload`'s own output. The conditions pack rides along because a
/// zero-document pack of either kind fails the run (FR-12).
///
/// # Panics
///
/// Panics if the checked-in tier seed is unreadable.
async fn imported_pool() -> Option<sqlx::PgPool> {
    let pool = testing::test_pool().await?;
    let zip = testing::pack_zip(&[
        ("packs/conditions.json", captured_conditions_pack()),
        ("packs/equipment.json", captured_item_pack()),
    ]);
    import_from_bytes(&pool, RELEASE, &zip, &tier_seed())
        .await
        .expect("the captured fixture imports cleanly");
    Some(pool)
}

/// Pull the JSON body out of the route's response.
async fn body_of(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body reads");
    serde_json::from_slice(&bytes).expect("the route answers JSON")
}

/// The release stamp for these runs — a test-only value; the importer only
/// records it on the row and no assertion here reads it back.
const RELEASE: &str = "pf2e-test-release";

/// The importer's stored shape and the route's literal path must agree on the
/// same fact: the captured pack's price sits exactly where the lookup reads.
/// A verbatim upstream capture on one side, the route's path on the other.
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
        "E12's book-value lookup reads data->'upstream'->'system'->'price'->'value'; \
         the importer's captured wayfinder price must sit at that path"
    );
    testing::drop_test_db(pool, "e12_bv_importer_path").await;
}

/// The whole chain: import the captured pack through the real write-path, ask
/// the route's lookup half for that item, and read the route's own body — a
/// row written by `store::apply_category`, not by this suite, resolves to its
/// book value.
#[tokio::test]
async fn the_route_answers_for_an_item_the_importer_wrote() {
    let Some(pool) = imported_pool().await else {
        return;
    };
    let response = crate::book_value::lookup(&pool, "Wayfinder").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_of(response).await,
        json!({"found": true, "price": captured_price()}),
        "the route resolves the importer's own row — a `found: false` here is \
         the D5-sketch defect: a stored-path belief the importer disagrees with"
    );
    testing::drop_test_db(pool, "e12_bv_importer_route").await;
}
