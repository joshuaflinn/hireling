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

/// Append one audit row and one structured log line.
///
/// A failed insert is logged at `error` and swallowed deliberately: the audit
/// record must never turn into a request failure that changes the caller's
/// verdict (the write being audited has already been allowed or rejected).
/// Silence would violate the observability rules, so the failure is loud.
pub async fn record(
    pool: &PgPool,
    event: AuditEvent,
    actor_sub: Option<&str>,
    target: &str,
    outcome: AuditOutcome,
    request_id: Option<&str>,
) {
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
    .execute(pool)
    .await;

    let span = tracing::info_span!("audit", request_id);
    match result {
        Ok(_) => tracing::info!(
            target: "audit",
            parent: &span,
            event = event.as_str(),
            actor = actor_sub.unwrap_or(""),
            subject = target,
            outcome = outcome.as_str(),
            "audit event"
        ),
        Err(err) => tracing::error!(
            target: "audit",
            parent: &span,
            error = %err,
            event = event.as_str(),
            actor = actor_sub.unwrap_or(""),
            subject = target,
            "failed to persist audit event"
        ),
    }
}
