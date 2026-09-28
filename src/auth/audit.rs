//! The audit trail: append-only records of logins and ownership-relevant
//! rejections (spec FR-15, SC-8). The `audit_events` table is authoritative;
//! a parallel `tracing` line with the same fields supports log-side diagnosis
//! and carries the request id.

use chrono::Utc;
use sqlx::PgPool;

/// The auditable events. The set is closed — the `audit_events.event` CHECK
/// constraint in the migration lists exactly these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEvent {
    LoginSuccess,
    LoginAllowlistDenied,
    Logout,
    ForbiddenCharacterWrite,
    ForbiddenEffectWrite,
    ForbiddenCustomWrite,
    ForbiddenGmWrite,
    /// One row per import attempt (E5, FR-16); admitted by migration
    /// 20260924000009 — the raised CHECK finding (design §5.2).
    CharacterImport,
}

impl AuditEvent {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuditEvent::LoginSuccess => "login_success",
            AuditEvent::LoginAllowlistDenied => "login_allowlist_denied",
            AuditEvent::Logout => "logout",
            AuditEvent::ForbiddenCharacterWrite => "forbidden_character_write",
            AuditEvent::ForbiddenEffectWrite => "forbidden_effect_write",
            AuditEvent::ForbiddenCustomWrite => "forbidden_custom_write",
            AuditEvent::ForbiddenGmWrite => "forbidden_gm_write",
            AuditEvent::CharacterImport => "character_import",
        }
    }
}

/// What came of the event, stored explicitly so ad-hoc queries need no case
/// table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    Allowed,
    Denied,
}

impl AuditOutcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuditOutcome::Allowed => "allowed",
            AuditOutcome::Denied => "denied",
        }
    }
}

/// Append one audit row and one structured log line, and report whether the
/// row landed. The authoritative variant: callers whose spec contract
/// requires the record to exist (SC-8 — the login legs) fail closed when it
/// cannot be written. Takes any executor so it can run inside the caller's
/// transaction.
///
/// # Errors
///
/// Returns the insert error after logging it — the row is not persisted.
pub async fn try_record(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    event: AuditEvent,
    actor_sub: Option<&str>,
    target: &str,
    outcome: AuditOutcome,
    request_id: Option<&str>,
) -> sqlx::Result<()> {
    let now = Utc::now();
    let result = sqlx::query(
        "INSERT INTO audit_events (occurred_at, actor_sub, event, target, outcome, request_id) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(now)
    .bind(actor_sub)
    .bind(event.as_str())
    .bind(target)
    .bind(outcome.as_str())
    .bind(request_id)
    .execute(executor)
    .await;

    let span = tracing::info_span!("audit", request_id);
    match result {
        Ok(_) => {
            tracing::info!(
                target: "audit",
                parent: &span,
                event = event.as_str(),
                actor = actor_sub.unwrap_or(""),
                subject = target,
                outcome = outcome.as_str(),
                "audit event"
            );
            Ok(())
        }
        Err(err) => {
            tracing::error!(
                target: "audit",
                parent: &span,
                error = %err,
                event = event.as_str(),
                actor = actor_sub.unwrap_or(""),
                subject = target,
                "failed to persist audit event"
            );
            Err(err)
        }
    }
}

/// Append one audit row on a path where the verdict is already delivered and
/// must not change (logout, ownership rejections): the insert is best effort,
/// failures are logged loudly by [`try_record`] and swallowed here. The login
/// legs must not use this — they owe the record its existence (SC-8).
pub async fn record(
    pool: &PgPool,
    event: AuditEvent,
    actor_sub: Option<&str>,
    target: &str,
    outcome: AuditOutcome,
    request_id: Option<&str>,
) {
    // The failure was already logged by `try_record`; here it is deliberately
    // non-fatal, so the result is dropped rather than propagated.
    drop(try_record(pool, event, actor_sub, target, outcome, request_id).await);
}
