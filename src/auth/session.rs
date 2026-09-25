//! Sessions: the server-held binding between a browser and an account, and
//! the sole source of acting identity (spec FR-3).
//!
//! The cookie carries an opaque 256-bit id; everything else lives in Postgres,
//! so a restart or deploy never logs the table out and a logout (row delete)
//! kills the session everywhere. Lifecycle evaluation is pure; the store is
//! the only I/O.

use chrono::DateTime;
use chrono::Utc;
use sqlx::PgPool;

use crate::auth::authz::Role;

/// How long a session may sit idle before it expires (the sliding window).
pub const DEFAULT_IDLE_SECS: i64 = 86_400;
/// Hard cap on a session's total lifetime, regardless of activity.
pub const DEFAULT_ABSOLUTE_SECS: i64 = 7 * 86_400;
/// Renewal writes are amortized: a session is touched at most once per window
/// on the request path. Expiry correctness is unaffected — every request
/// checks `expires_at` regardless.
pub const RENEWAL_AMORTIZATION: chrono::Duration = chrono::Duration::minutes(5);

/// A loaded session joined with its account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub account_sub: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub last_seen_at: DateTime<Utc>,
    pub idle_expires_at: DateTime<Utc>,
    pub absolute_expires_at: DateTime<Utc>,
}

impl SessionRecord {
    /// An idle- or absolute-expired session is dead: every endpoint must
    /// reject it as unauthenticated (spec FR-4), and activity does not rescue
    /// the absolute cap.
    #[must_use]
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.idle_expires_at || now >= self.absolute_expires_at
    }

    /// Whether this request should also slide the idle window forward.
    #[must_use]
    pub fn needs_renewal(&self, now: DateTime<Utc>) -> bool {
        now.signed_duration_since(self.last_seen_at) > RENEWAL_AMORTIZATION
    }
}

/// Persist a new session row. Takes any executor so it can join the
/// caller's transaction (the login path writes the account, the session,
/// and the audit record as one unit).
///
/// # Errors
///
/// Returns an error if the insert fails (database unreachable, FK missing —
/// the account row must already exist).
pub async fn insert(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: &str,
    account_sub: &str,
    now: DateTime<Utc>,
    idle_secs: i64,
    absolute_secs: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO sessions (id, account_sub, created_at, last_seen_at, expires_at, \
         absolute_expires_at) VALUES ($1, $2, $3, $3, $4, $5)",
    )
    .bind(id)
    .bind(account_sub)
    .bind(now)
    .bind(now + chrono::Duration::seconds(idle_secs))
    .bind(now + chrono::Duration::seconds(absolute_secs))
    .execute(executor)
    .await
    .map_err(|err| anyhow::Error::new(err).context("failed to insert session"))?;
    Ok(())
}

/// Load a session joined with its account. `None` when the id is unknown —
/// including the never-distinguishable cases of a logged-out or reaped row.
///
/// # Errors
///
/// Returns an error if the query fails, or if the stored role is not a known
/// value — an unreadable role must fail the request, never silently widen it.
pub async fn find(pool: &PgPool, id: &str) -> anyhow::Result<Option<SessionRecord>> {
    let row = sqlx::query_as::<_, SessionRow>(
        "SELECT s.id, a.sub, a.username, a.display_name, a.role, \
                s.last_seen_at, s.expires_at, s.absolute_expires_at \
         FROM sessions s JOIN accounts a ON a.sub = s.account_sub WHERE s.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|err| anyhow::Error::new(err).context("failed to load session"))?;

    match row {
        Some(row) => Ok(Some(SessionRecord::try_from(row).map_err(|err| {
            anyhow::anyhow!("session has an unusable role: {err}")
        })?)),
        None => Ok(None),
    }
}

/// Delete a session row (logout / administrative invalidation). Returns
/// whether a row was actually deleted.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub async fn delete(pool: &PgPool, id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| anyhow::Error::new(err).context("failed to delete session"))?;
    Ok(result.rows_affected() > 0)
}

/// Slide the idle window: `last_seen_at = now`, `expires_at = now + idle`.
///
/// # Errors
///
/// Returns an error if the update fails.
pub async fn touch(
    pool: &PgPool,
    id: &str,
    now: DateTime<Utc>,
    idle_secs: i64,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE sessions SET last_seen_at = $2, expires_at = $3 WHERE id = $1")
        .bind(id)
        .bind(now)
        .bind(now + chrono::Duration::seconds(idle_secs))
        .execute(pool)
        .await
        .map_err(|err| anyhow::Error::new(err).context("failed to renew session"))?;
    Ok(())
}

/// The raw database shape — `sqlx` maps rows here, [`SessionRecord`] is what
/// the rest of the auth code sees.
#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    sub: String,
    username: String,
    display_name: String,
    role: String,
    last_seen_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    absolute_expires_at: DateTime<Utc>,
}

impl TryFrom<SessionRow> for SessionRecord {
    type Error = String;

    fn try_from(row: SessionRow) -> Result<Self, Self::Error> {
        let role =
            Role::from_db(&row.role).ok_or_else(|| format!("unknown role {:?}", row.role))?;
        Ok(Self {
            id: row.id,
            account_sub: row.sub,
            username: row.username,
            display_name: row.display_name,
            role,
            last_seen_at: row.last_seen_at,
            idle_expires_at: row.expires_at,
            absolute_expires_at: row.absolute_expires_at,
        })
    }
}

#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;
