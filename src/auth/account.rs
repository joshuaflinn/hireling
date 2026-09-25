//! Accounts: one row per allowlisted human, keyed by the provider's `sub`
//! (the cross-epic keying contract). Rows are upserted at login — nothing
//! else writes them; E2 owns the domain tables that reference this key.

use sqlx::PgPool;

use crate::auth::authz::Role;
use crate::auth::oidc::AccountIdentity;

/// Insert or refresh the account for a successfully authenticated identity.
/// `role` is recomputed on every login from the configured GM `sub`, so a GM
/// designation change in config takes effect at the next login.
///
/// # Errors
///
/// Returns an error if the write fails.
pub async fn upsert(pool: &PgPool, identity: &AccountIdentity, gm_sub: &str) -> anyhow::Result<()> {
    let role = if identity.sub == gm_sub {
        Role::Gm
    } else {
        Role::Player
    };
    sqlx::query(
        "INSERT INTO accounts (sub, username, display_name, role, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, now(), now()) \
         ON CONFLICT (sub) DO UPDATE SET \
           username = excluded.username, \
           display_name = excluded.display_name, \
           role = excluded.role, \
           updated_at = now()",
    )
    .bind(&identity.sub)
    .bind(&identity.username)
    .bind(&identity.display_name)
    .bind(role.as_str())
    .execute(pool)
    .await
    .map_err(|err| anyhow::Error::new(err).context("failed to upsert account"))?;
    Ok(())
}
