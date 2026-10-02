//! Integration tests for E8's REST reads (plan Task 9): the condition
//! picker, the party's active effects, and the derived bootstrap — each
//! through the real router (PR #30 rule). Party-readable (member or GM),
//! writes nowhere exist, E3's 403 payload for outsiders.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt as _;

use super::test_router;
use crate::auth::error::Forbidden;
use crate::auth::oidc;
use crate::sync::protocol::{FieldTarget, Outcome};
use crate::sync::write::{ClientOp, apply_write};
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

/// Seed a real-sheet member (the reference export through the E5 pipeline).
async fn seed_real_member(pool: &sqlx::PgPool, sub: &str) -> (i64, i64) {
    const REFERENCE: &str = include_str!("../../tests/data/pb_export_reference.json");
    let export = crate::pbimport::model::parse_and_validate(REFERENCE).expect("valid reference");
    let (sheet, skips) = crate::pbimport::transform::transform(&export);
    assert!(
        skips.sections.is_empty(),
        "no skips in the reference export"
    );
    let sheet_json = serde_json::to_value(&sheet).expect("sheet serializes");
    testing::seed_account(pool, sub, "player").await;
    let party_id: i64 = sqlx::query_scalar("SELECT id FROM parties ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("POC party");
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

async fn seed_condition(pool: &sqlx::PgPool, name: &str, tier: &str, valued: bool) -> i64 {
    let modifiers = if tier == "engine_math" {
        serde_json::json!([{
            "type": "status", "stat": "all_checks_and_dcs", "value": null,
            "value_kind": if valued { "condition_value" } else { "unreachable" },
            "polarity": "negative",
        }])
    } else {
        Value::Null
    };
    sqlx::query_scalar(
        "INSERT INTO corpus_entries (kind, name, lane, data, modifiers) \
         VALUES ('condition', $1, 'core', $2, $3) RETURNING id",
    )
    .bind(name)
    .bind(serde_json::json!({"import": {"tier": tier}}))
    .bind(modifiers)
    .fetch_one(pool)
    .await
    .expect("corpus row")
}

fn player_op(op_id: &str, party_id: i64, source: i64, targets: &[i64]) -> ClientOp {
    ClientOp {
        op_id: op_id.to_owned(),
        target: crate::sync::protocol::FieldTarget::EffectNew { party_id },
        base_version: 0,
        value: serde_json::json!({
            "op": "create", "name": "Bless", "source_character_id": source,
            "targets": targets,
            "modifiers": [{"type": "status", "stat": "attack", "value": 1}],
            "duration_note": "", "corpus_entry_id": null, "condition_value": null,
        }),
    }
}

#[tokio::test]
async fn the_picker_reads_the_corpus_not_the_code() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let valued: i64 = seed_condition(&pool, "Frightened", "engine_math", true).await;
    let flat: i64 = seed_condition(&pool, "Concealed", "display_only", false).await;
    let constant: i64 = seed_condition(&pool, "Inspired", "engine_math", false).await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let _ = get(&app, "/api/parties/1/conditions", &session).await;
    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/conditions"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().expect("a JSON array of picks");
    let find = |name: &str| {
        rows.iter()
            .find(|row| row.get("name").and_then(Value::as_str) == Some(name))
            .expect("pick row")
    };
    let frightened = find("Frightened");
    assert_eq!(
        frightened.get("tier").and_then(Value::as_str),
        Some("engine_math")
    );
    assert_eq!(
        frightened.get("valued"),
        Some(&Value::Bool(true)),
        "valued is read from the stored mappings"
    );
    assert_eq!(
        frightened.get("corpus_entry_id").and_then(Value::as_i64),
        Some(valued)
    );
    let concealed = find("Concealed");
    assert_eq!(
        concealed.get("tier").and_then(Value::as_str),
        Some("display_only")
    );
    assert_eq!(concealed.get("valued"), Some(&Value::Bool(false)));
    let inspired = find("Inspired");
    assert_eq!(
        inspired.get("valued"),
        Some(&Value::Bool(false)),
        "a constant mapping is not a valued condition"
    );
    let _ = (flat, constant);
    testing::drop_test_db(pool, "e8_rest_picker").await;
}

#[tokio::test]
async fn the_picker_refuses_an_outsider_with_the_e3_payload() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_real_member(&pool, "dev-sub-josh").await;
    testing::seed_account(&pool, "dev-sub-bear", "player").await;
    let outsider = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/conditions"),
        &outsider,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body,
        serde_json::from_str::<Value>(Forbidden::body()).expect("e3 shape")
    );
    testing::drop_test_db(pool, "e8_rest_picker_403").await;
}

#[tokio::test]
async fn the_gm_reads_the_picker_too() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, _) = seed_real_member(&pool, "dev-sub-josh").await;
    seed_condition(&pool, "Frightened", "engine_math", true).await;
    testing::seed_account(&pool, "dev-sub-gm", "gm").await;
    let gm_session = testing::seed_session(&pool, "dev-sub-gm", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(
        &app,
        &format!("/api/parties/{party_id}/conditions"),
        &gm_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GM reads succeed");
    assert!(
        body.as_array()
            .expect("array")
            .iter()
            .any(|row| row.get("name").and_then(Value::as_str) == Some("Frightened"))
    );
    testing::drop_test_db(pool, "e8_rest_gm_read").await;
}

#[tokio::test]
async fn the_effects_read_returns_actives_only() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let actor = crate::auth::authz::Actor {
        sub: "dev-sub-josh".to_owned(),
        role: crate::auth::authz::Role::Player,
    };

    let created = apply_write(
        &pool,
        &actor,
        party_id,
        player_op("fx-1", party_id, alpha, &[alpha]),
    )
    .await
    .expect("create");
    assert_eq!(created.outcome, Outcome::Applied);
    let Some((FieldTarget::Effect { effect_id }, _)) = created.broadcast else {
        panic!("create broadcasts");
    };
    // End a SECOND effect so the read proves the active filter.
    let second = apply_write(
        &pool,
        &actor,
        party_id,
        player_op("fx-2", party_id, alpha, &[alpha]),
    )
    .await
    .expect("second create");
    let Some((FieldTarget::Effect { effect_id: doomed }, _)) = second.broadcast else {
        panic!("second broadcasts");
    };
    let doomed_version: i64 = sqlx::query_scalar("SELECT version FROM effects WHERE id = $1")
        .bind(doomed)
        .fetch_one(&pool)
        .await
        .expect("version");
    let _ = doomed_version;
    sqlx::query("UPDATE effects SET active = false WHERE id = $1")
        .bind(doomed)
        .execute(&pool)
        .await
        .expect("end the second");

    let app = testing::router_for(pool.clone(), &testing::auth_settings());
    let (status, body) = get(&app, &format!("/api/parties/{party_id}/effects"), &session).await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().expect("array of effects");
    assert_eq!(rows.len(), 1, "only the active effect");
    assert_eq!(
        rows.first()
            .and_then(|row| row.get("effect_id"))
            .and_then(Value::as_i64),
        Some(effect_id)
    );
    assert_eq!(
        rows.first()
            .and_then(|row| row.get("active"))
            .and_then(Value::as_bool),
        Some(true)
    );
    testing::drop_test_db(pool, "e8_rest_effects").await;
}

#[tokio::test]
async fn the_derived_read_bootstraps_one_character() {
    let Some(pool) = testing::test_pool().await else {
        return;
    };
    let (_party_id, alpha) = seed_real_member(&pool, "dev-sub-josh").await;
    // The outsider sits in a DIFFERENT party — same auth, no read.
    testing::seed_account(&pool, "dev-sub-bear", "player").await;
    let other_party: i64 =
        sqlx::query_scalar("INSERT INTO parties (name) VALUES ('Elsewhere') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("second party");
    let outsider_character: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, 'dev-sub-bear', '{}', '{}') RETURNING id",
    )
    .bind(other_party)
    .fetch_one(&pool)
    .await
    .expect("outsider character");
    sqlx::query("INSERT INTO character_vitals (character_id) VALUES ($1)")
        .bind(outsider_character)
        .execute(&pool)
        .await
        .expect("outsider vitals");
    let session = testing::seed_session(&pool, "dev-sub-josh", chrono::Utc::now()).await;
    let outsider = testing::seed_session(&pool, "dev-sub-bear", chrono::Utc::now()).await;
    let app = testing::router_for(pool.clone(), &testing::auth_settings());

    let (status, body) = get(&app, &format!("/api/characters/{alpha}/derived"), &session).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.get("character_id").and_then(Value::as_i64),
        Some(alpha)
    );
    assert_eq!(
        body.get("schema").and_then(Value::as_str),
        Some("hireling.engine.output.v1"),
        "the bootstrap is the EngineOutput contract shape"
    );
    assert!(body.get("derived").is_some());
    assert!(body.get("effects").is_some());

    // An outsider is 403 even for a character id that exists.
    let (outsider_status, outsider_body) =
        get(&app, &format!("/api/characters/{alpha}/derived"), &outsider).await;
    assert_eq!(outsider_status, StatusCode::FORBIDDEN);
    assert_eq!(
        outsider_body,
        serde_json::from_str::<Value>(Forbidden::body()).expect("e3 shape")
    );

    // An unknown character is 404, not 403 — it names no party to gate.
    let (missing_status, _) = get(&app, "/api/characters/999999/derived", &session).await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    let _ = outsider_character;
    testing::drop_test_db(pool, "e8_rest_derived").await;
}

#[tokio::test]
async fn an_anonymous_request_is_401_on_every_engine_read() {
    let app = test_router();
    for path in [
        "/api/parties/1/conditions",
        "/api/parties/1/effects",
        "/api/characters/1/derived",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{path} sits behind require_auth"
        );
    }
}
