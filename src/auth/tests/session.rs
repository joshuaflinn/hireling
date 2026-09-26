//! Tests for [`super`] — lifecycle evaluation is pure and always runs; the
//! store tests need a Postgres and skip loudly without one
//! (`crate::testing`).

use chrono::Duration;
use chrono::Utc;

use super::RENEWAL_AMORTIZATION;
use super::SessionRecord;

fn record(last_seen_age_secs: i64, idle_in_secs: i64, absolute_in_secs: i64) -> SessionRecord {
    let now = Utc::now();
    SessionRecord {
        id: "test-session".to_owned(),
        account_sub: "sub".to_owned(),
        username: "sub".to_owned(),
        display_name: "sub".to_owned(),
        role: crate::auth::authz::Role::Player,
        last_seen_at: now - Duration::seconds(last_seen_age_secs),
        idle_expires_at: now + Duration::seconds(idle_in_secs),
        absolute_expires_at: now + Duration::seconds(absolute_in_secs),
    }
}

#[test]
fn a_fresh_session_is_active_and_does_not_need_renewal() {
    let session = record(0, 86_400, 7 * 86_400);
    assert!(!session.is_expired(Utc::now()));
    assert!(!session.needs_renewal(Utc::now()));
}

#[test]
fn an_idle_expired_session_is_dead_even_moments_after() {
    let session = record(0, 0, 7 * 86_400);
    assert!(
        session.is_expired(Utc::now()),
        "expires_at == now means expired: no race windows on session lifetime"
    );
}

#[test]
fn activity_does_not_rescue_the_absolute_cap() {
    let session = record(0, 3_600, 0);
    assert!(
        session.is_expired(Utc::now()),
        "absolute_expires_at reached: renewal must not extend past the hard cap"
    );
}

#[test]
fn renewal_is_amortized() {
    assert!(
        !record(60, 86_400, 7 * 86_400).needs_renewal(Utc::now()),
        "a request a minute after the last touch must not write the row"
    );
    assert!(
        !record(RENEWAL_AMORTIZATION.num_seconds() - 1, 86_400, 7 * 86_400)
            .needs_renewal(Utc::now()),
        "at the boundary, no renewal"
    );
    assert!(
        record(RENEWAL_AMORTIZATION.num_seconds() + 1, 86_400, 7 * 86_400)
            .needs_renewal(Utc::now()),
        "beyond the amortization window, the idle window slides"
    );
}

// --- store ----------------------------------------------------------------

mod store {
    use super::super::delete;
    use super::super::find;
    use super::super::insert;
    use super::super::touch;
    use chrono::Duration;
    use chrono::Utc;

    #[tokio::test]
    async fn a_session_round_trips_through_the_store() {
        let Some(pool) = crate::testing::test_pool().await else {
            return;
        };
        crate::testing::seed_account(&pool, "sub-a", "player").await;
        let now = Utc::now();
        insert(&pool, "sess-1", "sub-a", now, 86_400, 7 * 86_400)
            .await
            .expect("insert");

        let loaded = find(&pool, "sess-1").await.expect("find").expect("exists");
        assert_eq!(loaded.id, "sess-1");
        assert_eq!(loaded.account_sub, "sub-a");
        assert_eq!(loaded.username, "sub-a");
        assert_eq!(loaded.role, crate::auth::authz::Role::Player);
        assert!(!loaded.is_expired(Utc::now()));

        // Unknown ids look exactly like logged-out ones: no row, no error.
        let missing = find(&pool, "no-such-session").await.expect("find");
        assert!(missing.is_none());

        delete(&pool, "sess-1").await.expect("delete");
        assert!(find(&pool, "sess-1").await.expect("find").is_none());
        crate::testing::drop_test_db(pool, "store_round_trip").await;
    }

    #[tokio::test]
    async fn touch_slides_the_idle_window() {
        let Some(pool) = crate::testing::test_pool().await else {
            return;
        };
        crate::testing::seed_account(&pool, "sub-a", "player").await;
        let ten_minutes_ago = Utc::now() - Duration::minutes(10);
        insert(&pool, "sess-2", "sub-a", ten_minutes_ago, 3_600, 7 * 86_400)
            .await
            .expect("insert");

        let now = Utc::now();
        touch(&pool, "sess-2", now, 3_600).await.expect("touch");
        let loaded = find(&pool, "sess-2").await.expect("find").expect("exists");
        assert!(
            loaded.last_seen_at > ten_minutes_ago,
            "last_seen_at slid forward"
        );
        assert!(loaded.idle_expires_at > now + Duration::seconds(3_590));
        crate::testing::drop_test_db(pool, "store_touch").await;
    }

    #[tokio::test]
    async fn a_role_the_store_cannot_read_fails_the_lookup() {
        let Some(pool) = crate::testing::test_pool().await else {
            return;
        };
        // The CHECK constraints make this unreachable through normal writes;
        // the test proves the failure mode directly by simulating drift.
        sqlx::query("ALTER TABLE accounts DROP CONSTRAINT accounts_role_check")
            .execute(&pool)
            .await
            .expect("drop role check");
        sqlx::query("ALTER TABLE sessions DROP CONSTRAINT sessions_account_sub_fkey")
            .execute(&pool)
            .await
            .expect("drop session fk");
        sqlx::query(
            "INSERT INTO accounts (sub, username, display_name, role) VALUES ('sub-b', 'sub-b', 'sub-b', 'overlord')",
        )
        .execute(&pool)
        .await
        .expect("seed corrupted account");
        let now = Utc::now();
        insert(&pool, "sess-3", "sub-b", now, 3_600, 7 * 86_400)
            .await
            .expect("insert");

        let loaded = find(&pool, "sess-3").await;
        assert!(
            loaded.is_err(),
            "an unreadable role must fail the request, never silently widen it"
        );
        crate::testing::drop_test_db(pool, "store_bad_role").await;
    }
}
