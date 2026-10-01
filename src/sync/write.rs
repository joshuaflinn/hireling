//! The per-field CAS write engine — E7's core (plan Task 3).
//!
//! Bounds → target resolution → party scope → E3 authorization → ledger
//! reservation → CAS → ledger finalize, one transaction. `superseded` is a
//! *fair race loss* (the field moved past `base_version`); `rejected` is bad
//! input that never touched a row; `forbidden` is a denial (E3's matrix or
//! the socket's bound party, contract §4). Every outcome is recorded
//! durably in `client_ops` — `already_applied` is a read answer, never a
//! stored row (data-model.md).

use anyhow::Context as _;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Transaction};

use crate::auth::authz::{Action, Actor, Resource, Role, Verdict, authorize};
use crate::sync::protocol::{FieldTarget, Outcome, VitalsField};

/// One client operation: the decoded `write` frame's payload.
pub struct ClientOp {
    pub op_id: String,
    pub target: FieldTarget,
    pub base_version: i64,
    pub value: JsonValue,
}

/// The authoritative answer for one operation. `version` is the field
/// version this op produced (applied / already-applied replays);
/// `winning_version` is the field's current version after a lost race;
/// `reason` carries the rejection/forbidden text for the ack.
pub struct WriteResult {
    pub outcome: Outcome,
    pub version: Option<i64>,
    pub winning_version: Option<i64>,
    pub reason: Option<String>,
}

/// The row a write targets, resolved fresh per operation — ownership comes
/// from the database, never from the frame (E3's rule); `party_id` is the
/// character's party, the write's addressing scope (contract §1). `None`
/// only on the unreachable effect arm (bounds rejects effects first).
struct TargetRow {
    owner_sub: String,
    party_id: Option<i64>,
    field_path: String,
}

/// Apply one client operation on behalf of a socket bound to `party_id` —
/// every write is scoped to the socket's party before ownership is even
/// asked.
///
/// # Errors
///
/// Database failures propagate (`anyhow`); every *protocol* outcome —
/// applied, superseded, already applied, rejected, forbidden — is a
/// [`WriteResult`], never an error.
pub async fn apply_write(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    op: ClientOp,
) -> anyhow::Result<WriteResult> {
    // 1. Bounds: pure, before anything touches the database. Rejected means
    //    never applied — distinct from losing a fair race.
    if let Err(reason) = validate_bounds(&op.target, &op.value) {
        return record_denial(pool, actor, &op, Outcome::Rejected, reason).await;
    }

    // 2. Resolve the target row (existence, owner, party). Unknown targets
    //    are rejected, not superseded (contract §4).
    let Some(target) = resolve_target(pool, &op.target).await? else {
        return record_denial(
            pool,
            actor,
            &op,
            Outcome::Rejected,
            "unknown target".to_owned(),
        )
        .await;
    };

    // 3. The bound party scopes the write (contract §1: one socket, one
    //    party). A target outside it is forbidden even when the writer owns
    //    it — a cross-party write would commit data its party never sees
    //    and leak it into a party that does not own it. Party scope runs
    //    before ownership: it is the frame's addressing scope.
    if target.party_id != Some(party_id) {
        return record_denial(
            pool,
            actor,
            &op,
            Outcome::Forbidden,
            "target character is not in this party".to_owned(),
        )
        .await;
    }

    // 4. E3's matrix, per message, deny-by-default (spec FR-1).
    let resource = Resource::Character {
        owner_sub: target.owner_sub,
    };
    if authorize(actor, Action::Write, &resource) == Verdict::Deny {
        let reason = if actor.role == Role::Gm {
            "gm is read-only"
        } else {
            "not the character's owner"
        };
        return record_denial(pool, actor, &op, Outcome::Forbidden, reason.to_owned()).await;
    }

    // 5. One transaction: ledger reservation → CAS → ledger finalize
    //    (design §6 steps 3–6; review-hardened). The reservation BEFORE the
    //    CAS serializes the op_id through its primary key: a concurrent
    //    twin with the same id blocks on the reservation, then answers from
    //    the holder's committed row — never a second committed write under
    //    one id. The durable ledger is what makes ack-loss replays
    //    exactly-once across restarts (spec FR-8).
    let mut tx = pool.begin().await.context("begin write transaction")?;
    let request = request_json(&op);
    let reserved =
        reserve_ledger(&mut tx, &op.op_id, &actor.sub, &target.field_path, &request).await?;
    if reserved.is_none() {
        tx.rollback().await.context("rollback id race")?;
        return answer_replay(pool, &op.op_id, &request).await;
    }

    let (outcome, version, winning_version) =
        cas(&mut tx, &op.target, op.base_version, &op.value).await?;
    finalize_ledger(&mut tx, &op.op_id, outcome, version).await?;
    tx.commit().await.context("commit write")?;

    Ok(WriteResult {
        outcome,
        version,
        winning_version,
        reason: None,
    })
}

/// Reserve the `op_id` inside the write transaction: the primary key is the
/// serialization point. The placeholder row is uncommitted until the
/// caller's finalize — on conflict the insert waits for the concurrent
/// holder and then yields, so the loser can answer from the holder's row
/// instead of committing a second write under the same id.
async fn reserve_ledger(
    tx: &mut Transaction<'_, Postgres>,
    op_id: &str,
    account_sub: &str,
    field_path: &str,
    request: &JsonValue,
) -> anyhow::Result<Option<String>> {
    let reserved: Option<String> = sqlx::query_scalar(
        "INSERT INTO client_ops (op_id, account_sub, field_path, request, outcome, \
         resulting_version) VALUES ($1, $2, $3, $4, 'applied', NULL) \
         ON CONFLICT (op_id) DO NOTHING RETURNING op_id",
    )
    .bind(op_id)
    .bind(account_sub)
    .bind(field_path)
    .bind(request)
    .fetch_optional(&mut **tx)
    .await
    .context("ledger reservation")?;
    Ok(reserved)
}

/// The `op_id` was held by a concurrent transaction. A genuine replay — the
/// SAME request resent — is answered `already_applied` from the holder's
/// row; reuse for anything else is the client bug data-model.md says the PK
/// catches loudly: rejected, nothing committed, the holder's row standing.
async fn answer_replay(
    pool: &PgPool,
    op_id: &str,
    request: &JsonValue,
) -> anyhow::Result<WriteResult> {
    let (stored_outcome, stored_version, stored_request): (String, Option<i64>, JsonValue) =
        sqlx::query_as(
            "SELECT outcome, resulting_version, request FROM client_ops WHERE op_id = $1",
        )
        .bind(op_id)
        .fetch_one(pool)
        .await
        .context("ledger row of the id holder")?;
    if stored_request == *request {
        return Ok(WriteResult {
            outcome: Outcome::AlreadyApplied,
            version: stored_version,
            winning_version: None,
            reason: None,
        });
    }
    tracing::warn!(
        op_id,
        held_outcome = %stored_outcome,
        "op_id reused for a different request; rejecting"
    );
    Ok(WriteResult {
        outcome: Outcome::Rejected,
        version: None,
        winning_version: None,
        reason: Some("op_id was already used for a different request".to_owned()),
    })
}

/// Replace the reservation's placeholder with the CAS's terminal outcome —
/// still inside the same transaction, so no committed row ever shows a
/// placeholder.
async fn finalize_ledger(
    tx: &mut Transaction<'_, Postgres>,
    op_id: &str,
    outcome: Outcome,
    resulting_version: Option<i64>,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE client_ops SET outcome = $1, resulting_version = $2 WHERE op_id = $3")
        .bind(outcome.as_str())
        .bind(resulting_version)
        .bind(op_id)
        .execute(&mut **tx)
        .await
        .context("ledger finalize")?;
    Ok(())
}

/// The ledger's canonical record of one request: `{target, base_version,
/// value}` — the replay-vs-reuse comparison key (data-model.md).
fn request_json(op: &ClientOp) -> JsonValue {
    serde_json::json!({
        "target": op.target,
        "base_version": op.base_version,
        "value": op.value,
    })
}

/// Record a denial (rejected / forbidden) durably and build its result.
/// Nothing else is touched — a denial writes one ledger row, never a field.
async fn record_denial(
    pool: &PgPool,
    actor: &Actor,
    op: &ClientOp,
    outcome: Outcome,
    reason: String,
) -> anyhow::Result<WriteResult> {
    let mut tx = pool.begin().await.context("begin denial transaction")?;
    insert_ledger(
        &mut tx,
        &op.op_id,
        &actor.sub,
        &field_path(&op.target),
        &request_json(op),
        outcome,
        None,
    )
    .await?;
    tx.commit().await.context("commit denial")?;
    Ok(WriteResult {
        outcome,
        version: None,
        winning_version: None,
        reason: Some(reason),
    })
}

/// Resolve the target's owner, party, and ledger field path, or `None` when
/// no such row exists for the addressing character.
async fn resolve_target(pool: &PgPool, target: &FieldTarget) -> anyhow::Result<Option<TargetRow>> {
    match target {
        FieldTarget::Vitals {
            character_id,
            field,
        } => {
            let row: Option<(String, Option<i64>)> = sqlx::query_as(
                "SELECT c.owner_sub, c.party_id FROM characters c \
                 JOIN character_vitals v ON v.character_id = c.id WHERE c.id = $1",
            )
            .bind(character_id)
            .fetch_optional(pool)
            .await
            .context("resolve vitals target")?;
            Ok(row.map(|(owner_sub, party_id)| TargetRow {
                owner_sub,
                party_id,
                field_path: format!("vitals:{}", field.as_str()),
            }))
        }
        FieldTarget::Slot {
            character_id,
            caster_key,
            rank,
            slot_index,
        } => {
            let row: Option<(String, Option<i64>)> = sqlx::query_as(
                "SELECT c.owner_sub, c.party_id FROM character_spell_slots s \
                 JOIN characters c ON c.id = s.character_id \
                 WHERE s.character_id = $1 AND s.caster_key = $2 \
                   AND s.rank = $3 AND s.slot_index = $4",
            )
            .bind(character_id)
            .bind(caster_key)
            .bind(rank)
            .bind(slot_index)
            .fetch_optional(pool)
            .await
            .context("resolve slot target")?;
            Ok(row.map(|(owner_sub, party_id)| TargetRow {
                owner_sub,
                party_id,
                field_path: format!("slot:{caster_key}:{rank}:{slot_index}"),
            }))
        }
        FieldTarget::Inv {
            character_id,
            item_name,
        } => {
            let row: Option<(String, Option<i64>)> = sqlx::query_as(
                "SELECT c.owner_sub, c.party_id FROM character_inventory_live i \
                 JOIN characters c ON c.id = i.character_id \
                 WHERE i.character_id = $1 AND i.item_name = $2",
            )
            .bind(character_id)
            .bind(item_name)
            .fetch_optional(pool)
            .await
            .context("resolve inv target")?;
            Ok(row.map(|(owner_sub, party_id)| TargetRow {
                owner_sub,
                party_id,
                field_path: format!("inv:{item_name}"),
            }))
        }
        // Bounds rejects effect targets before resolution; this arm exists so
        // the match is honest. Unreachable through `apply_write`.
        FieldTarget::Effect { effect_id } => Ok(Some(TargetRow {
            owner_sub: String::new(),
            party_id: None,
            field_path: format!("effect:{effect_id}"),
        })),
    }
}

/// The compare-and-set write for one target kind. On a rowcount of 0 the
/// field moved (or the row is gone behind a still-valid owner): re-read the
/// current version and report [`Outcome::Superseded`] with it. Returns
/// `(outcome, version, winning_version)`.
///
/// # Errors
///
/// Database failures only — the CAS miss is a [`WriteResult`], not an error.
async fn cas(
    tx: &mut Transaction<'_, Postgres>,
    target: &FieldTarget,
    base_version: i64,
    value: &JsonValue,
) -> anyhow::Result<(Outcome, Option<i64>, Option<i64>)> {
    match target {
        FieldTarget::Vitals {
            character_id,
            field,
        } => cas_vitals(tx, *character_id, *field, base_version, value).await,
        FieldTarget::Slot {
            character_id,
            caster_key,
            rank,
            slot_index,
        } => {
            cas_slot(
                tx,
                *character_id,
                caster_key,
                *rank,
                *slot_index,
                base_version,
                value,
            )
            .await
        }
        FieldTarget::Inv {
            character_id,
            item_name,
        } => cas_inv(tx, *character_id, item_name, base_version, value).await,
        // Bounds rejects effect targets before CAS; unreachable here.
        FieldTarget::Effect { .. } => Ok((Outcome::Rejected, None, None)),
    }
}

/// CAS one vitals column; money writes set all four denominations under the
/// one `money_version` (E2's schema).
async fn cas_vitals(
    tx: &mut Transaction<'_, Postgres>,
    character_id: i64,
    field: VitalsField,
    base_version: i64,
    value: &JsonValue,
) -> anyhow::Result<(Outcome, Option<i64>, Option<i64>)> {
    let new_version: Option<i64> = match field {
        VitalsField::Hp => sqlx::query_scalar(
            "UPDATE character_vitals SET hp = $1, \
                 hp_version = nextval('field_version_seq'), updated_at = now() \
                 WHERE character_id = $2 AND hp_version = $3 RETURNING hp_version",
        )
        .bind(as_i32(value))
        .bind(character_id)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS hp")?,
        VitalsField::TempHp => sqlx::query_scalar(
            "UPDATE character_vitals SET temp_hp = $1, \
                 temp_hp_version = nextval('field_version_seq'), updated_at = now() \
                 WHERE character_id = $2 AND temp_hp_version = $3 \
                 RETURNING temp_hp_version",
        )
        .bind(as_i32(value))
        .bind(character_id)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS temp_hp")?,
        VitalsField::LevelAdjust => sqlx::query_scalar(
            "UPDATE character_vitals SET level_adjust = $1, \
                 level_adjust_version = nextval('field_version_seq'), updated_at = now() \
                 WHERE character_id = $2 AND level_adjust_version = $3 \
                 RETURNING level_adjust_version",
        )
        .bind(as_i32(value))
        .bind(character_id)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS level_adjust")?,
        VitalsField::Money => {
            let money = MoneyValue::from_json(value);
            sqlx::query_scalar(
                "UPDATE character_vitals SET money_pp = $1, money_gp = $2, \
                 money_sp = $3, money_cp = $4, \
                 money_version = nextval('field_version_seq'), updated_at = now() \
                 WHERE character_id = $5 AND money_version = $6 \
                 RETURNING money_version",
            )
            .bind(money.pp)
            .bind(money.gp)
            .bind(money.sp)
            .bind(money.cp)
            .bind(character_id)
            .bind(base_version)
            .fetch_optional(&mut **tx)
            .await
            .context("CAS money")?
        }
        VitalsField::FocusCurrent => sqlx::query_scalar(
            "UPDATE character_vitals SET focus_current = $1, \
                 focus_version = nextval('field_version_seq'), updated_at = now() \
                 WHERE character_id = $2 AND focus_version = $3 \
                 RETURNING focus_version",
        )
        .bind(as_i32(value))
        .bind(character_id)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS focus_current")?,
        VitalsField::HeroPoints => sqlx::query_scalar(
            "UPDATE character_vitals SET hero_points = $1, \
                 hero_points_version = nextval('field_version_seq'), updated_at = now() \
                 WHERE character_id = $2 AND hero_points_version = $3 \
                 RETURNING hero_points_version",
        )
        .bind(as_i32(value))
        .bind(character_id)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS hero_points")?,
        VitalsField::Daily => sqlx::query_scalar(
            "UPDATE character_vitals SET daily = $1, \
                 daily_version = nextval('field_version_seq'), updated_at = now() \
                 WHERE character_id = $2 AND daily_version = $3 \
                 RETURNING daily_version",
        )
        .bind(value)
        .bind(character_id)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS daily")?,
    };
    let Some(version) = new_version else {
        let winning = read_vitals_version(tx, character_id, field).await?;
        return Ok((Outcome::Superseded, None, Some(winning)));
    };
    Ok((Outcome::Applied, Some(version), None))
}

/// CAS one spell-slot row: a whole-slot write — unmentioned fields reset
/// (contract §3).
async fn cas_slot(
    tx: &mut Transaction<'_, Postgres>,
    character_id: i64,
    caster_key: &str,
    rank: i32,
    slot_index: i32,
    base_version: i64,
    value: &JsonValue,
) -> anyhow::Result<(Outcome, Option<i64>, Option<i64>)> {
    let used = value
        .get("used")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let prepared = match value.get("prepared") {
        Some(JsonValue::String(prepared)) => Some(prepared.clone()),
        _ => None,
    };
    let new_version: Option<i64> = sqlx::query_scalar(
        "UPDATE character_spell_slots SET used = $1, prepared_spell = $2, \
         version = nextval('field_version_seq'), updated_at = now() \
         WHERE character_id = $3 AND caster_key = $4 AND rank = $5 \
           AND slot_index = $6 AND version = $7 RETURNING version",
    )
    .bind(used)
    .bind(prepared)
    .bind(character_id)
    .bind(caster_key)
    .bind(rank)
    .bind(slot_index)
    .bind(base_version)
    .fetch_optional(&mut **tx)
    .await
    .context("CAS slot")?;
    let Some(version) = new_version else {
        let winning: i64 = sqlx::query_scalar(
            "SELECT version FROM character_spell_slots \
             WHERE character_id = $1 AND caster_key = $2 AND rank = $3 \
               AND slot_index = $4",
        )
        .bind(character_id)
        .bind(caster_key)
        .bind(rank)
        .bind(slot_index)
        .fetch_one(&mut **tx)
        .await
        .context("re-read slot version")?;
        return Ok((Outcome::Superseded, None, Some(winning)));
    };
    Ok((Outcome::Applied, Some(version), None))
}

/// CAS one inventory row. `qty_delta` is the row's new stored delta —
/// absolute set semantics like every other target (ledger ruling, hb1).
async fn cas_inv(
    tx: &mut Transaction<'_, Postgres>,
    character_id: i64,
    item_name: &str,
    base_version: i64,
    value: &JsonValue,
) -> anyhow::Result<(Outcome, Option<i64>, Option<i64>)> {
    let qty_delta = as_i32(value.get("qty_delta").unwrap_or(&JsonValue::Null));
    let new_version: Option<i64> = sqlx::query_scalar(
        "UPDATE character_inventory_live SET qty_delta = $1, \
         version = nextval('field_version_seq'), updated_at = now() \
         WHERE character_id = $2 AND item_name = $3 AND version = $4 \
         RETURNING version",
    )
    .bind(qty_delta)
    .bind(character_id)
    .bind(item_name)
    .bind(base_version)
    .fetch_optional(&mut **tx)
    .await
    .context("CAS inv")?;
    let Some(version) = new_version else {
        let winning: i64 = sqlx::query_scalar(
            "SELECT version FROM character_inventory_live \
             WHERE character_id = $1 AND item_name = $2",
        )
        .bind(character_id)
        .bind(item_name)
        .fetch_one(&mut **tx)
        .await
        .context("re-read inv version")?;
        return Ok((Outcome::Superseded, None, Some(winning)));
    };
    Ok((Outcome::Applied, Some(version), None))
}

/// Re-read one vitals version column after a CAS miss.
async fn read_vitals_version(
    tx: &mut Transaction<'_, Postgres>,
    character_id: i64,
    field: VitalsField,
) -> anyhow::Result<i64> {
    match field {
        VitalsField::Hp => {
            sqlx::query_scalar("SELECT hp_version FROM character_vitals WHERE character_id = $1")
                .bind(character_id)
                .fetch_one(&mut **tx)
                .await
                .context("re-read hp_version")
        }
        VitalsField::TempHp => sqlx::query_scalar(
            "SELECT temp_hp_version FROM character_vitals WHERE character_id = $1",
        )
        .bind(character_id)
        .fetch_one(&mut **tx)
        .await
        .context("re-read temp_hp_version"),
        VitalsField::LevelAdjust => sqlx::query_scalar(
            "SELECT level_adjust_version FROM character_vitals WHERE character_id = $1",
        )
        .bind(character_id)
        .fetch_one(&mut **tx)
        .await
        .context("re-read level_adjust_version"),
        VitalsField::Money => {
            sqlx::query_scalar("SELECT money_version FROM character_vitals WHERE character_id = $1")
                .bind(character_id)
                .fetch_one(&mut **tx)
                .await
                .context("re-read money_version")
        }
        VitalsField::FocusCurrent => {
            sqlx::query_scalar("SELECT focus_version FROM character_vitals WHERE character_id = $1")
                .bind(character_id)
                .fetch_one(&mut **tx)
                .await
                .context("re-read focus_version")
        }
        VitalsField::HeroPoints => sqlx::query_scalar(
            "SELECT hero_points_version FROM character_vitals WHERE character_id = $1",
        )
        .bind(character_id)
        .fetch_one(&mut **tx)
        .await
        .context("re-read hero_points_version"),
        VitalsField::Daily => {
            sqlx::query_scalar("SELECT daily_version FROM character_vitals WHERE character_id = $1")
                .bind(character_id)
                .fetch_one(&mut **tx)
                .await
                .context("re-read daily_version")
        }
    }
}

/// Append one denial outcome to the durable ledger. `ON CONFLICT DO
/// NOTHING`: the id's first disposition stands (denials write no fields, so
/// a lost row loses nothing but the duplicate).
async fn insert_ledger(
    tx: &mut Transaction<'_, Postgres>,
    op_id: &str,
    account_sub: &str,
    field_path: &str,
    request: &JsonValue,
    outcome: Outcome,
    resulting_version: Option<i64>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO client_ops (op_id, account_sub, field_path, request, outcome, resulting_version) \
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (op_id) DO NOTHING",
    )
    .bind(op_id)
    .bind(account_sub)
    .bind(field_path)
    .bind(request)
    .bind(outcome.as_str())
    .bind(resulting_version)
    .execute(&mut **tx)
    .await
    .context("ledger insert")?;
    Ok(())
}

/// The ledger's canonical field-path text for a target (data-model.md):
/// `vitals:hp`, `slot:Wizard:3:0`, `inv:Chalk`, `effect:42`.
fn field_path(target: &FieldTarget) -> String {
    match target {
        FieldTarget::Vitals { field, .. } => format!("vitals:{}", field.as_str()),
        FieldTarget::Slot {
            caster_key,
            rank,
            slot_index,
            ..
        } => format!("slot:{caster_key}:{rank}:{slot_index}"),
        FieldTarget::Inv { item_name, .. } => format!("inv:{item_name}"),
        FieldTarget::Effect { effect_id } => format!("effect:{effect_id}"),
    }
}

/// The bounds table (plan T3): reject, never silently fix. Pure.
fn validate_bounds(target: &FieldTarget, value: &JsonValue) -> Result<(), String> {
    match target {
        FieldTarget::Vitals {
            character_id: _,
            field,
        } => match field {
            VitalsField::Hp => int_in_range(value, 0, i32::MAX, "hp"),
            VitalsField::TempHp => int_in_range(value, 0, i32::MAX, "temp_hp"),
            VitalsField::LevelAdjust => int_in_range(value, -19, 19, "level_adjust"),
            VitalsField::Money => {
                let Some(object) = value.as_object() else {
                    return Err("money must be an object {pp, gp, sp, cp}".to_owned());
                };
                if object.len() != 4 {
                    return Err("money needs exactly pp, gp, sp, cp".to_owned());
                }
                for key in ["pp", "gp", "sp", "cp"] {
                    match object.get(key).and_then(JsonValue::as_i64) {
                        Some(amount) if (0..=i64::from(i32::MAX)).contains(&amount) => {}
                        _ => return Err(format!("money.{key} must be an integer \u{2265} 0")),
                    }
                }
                Ok(())
            }
            // The server validates the shape and its own bounds only — the
            // focus max is the character's, so the clamp is client-side
            // (wire-protocol §3).
            VitalsField::FocusCurrent => int_in_range(value, 0, i32::MAX, "focus_current"),
            VitalsField::HeroPoints => int_in_range(value, 0, i32::MAX, "hero_points"),
            VitalsField::Daily => {
                let Some(object) = value.as_object() else {
                    return Err(
                        "daily must be an object {staff_charge_rank, staff_spent, drain_used}"
                            .to_owned(),
                    );
                };
                if object.len() != 3 {
                    return Err(
                        "daily needs exactly staff_charge_rank, staff_spent, drain_used".to_owned(),
                    );
                }
                match object.get("staff_charge_rank").and_then(JsonValue::as_i64) {
                    Some(rank) if (0..=10).contains(&rank) => {}
                    _ => {
                        return Err(
                            "daily.staff_charge_rank must be an integer within 0\u{2014}10"
                                .to_owned(),
                        );
                    }
                }
                match object.get("staff_spent").and_then(JsonValue::as_i64) {
                    Some(spent) if (0..=i64::from(i32::MAX)).contains(&spent) => {}
                    _ => return Err("daily.staff_spent must be an integer \u{2265} 0".to_owned()),
                }
                if !object.get("drain_used").is_some_and(JsonValue::is_boolean) {
                    return Err("daily.drain_used must be a boolean".to_owned());
                }
                Ok(())
            }
        },
        FieldTarget::Slot { .. } => {
            let Some(object) = value.as_object() else {
                return Err("slot value must be an object".to_owned());
            };
            for key in object.keys() {
                if !matches!(key.as_str(), "used" | "prepared") {
                    return Err(format!("slot value: unknown key {key:?}"));
                }
            }
            if let Some(used) = object.get("used")
                && !used.is_boolean()
            {
                return Err("slot.used must be a boolean".to_owned());
            }
            match object.get("prepared") {
                None | Some(JsonValue::Null) => {}
                Some(prepared) if prepared.is_string() => {}
                Some(_) => return Err("slot.prepared must be a string or null".to_owned()),
            }
            Ok(())
        }
        FieldTarget::Inv { .. } => {
            let Some(object) = value.as_object() else {
                return Err("inv value must be {qty_delta: <integer>}".to_owned());
            };
            if object.len() != 1 || !object.contains_key("qty_delta") {
                return Err("inv value must be exactly {qty_delta: <integer>}".to_owned());
            }
            match object
                .get("qty_delta")
                .and_then(JsonValue::as_i64)
                .and_then(|delta| i32::try_from(delta).ok())
            {
                Some(_) => Ok(()),
                None => Err("qty_delta must be an integer".to_owned()),
            }
        }
        // Decode denies effect writes; a direct call is refused too — the
        // engine must not write effects even if a future caller skips decode.
        FieldTarget::Effect { .. } => Err("effect writes are deferred to E8".to_owned()),
    }
}

/// One integer value inside `[min, max]`, with a reason naming the bound.
fn int_in_range(value: &JsonValue, min: i32, max: i32, name: &str) -> Result<(), String> {
    match value.as_i64() {
        Some(number) if number >= i64::from(min) && number <= i64::from(max) => Ok(()),
        Some(_) => Err(format!("{name} must be within {min}\u{2014}{max}")),
        None => Err(format!("{name} must be an integer")),
    }
}

/// Coerce a bounds-checked integer; `None` only if bounds were bypassed
/// (the CAS then writes NULL-able? no — Postgres refuses the type mismatch,
/// and every caller runs `validate_bounds` first).
fn as_i32(value: &JsonValue) -> Option<i32> {
    value.as_i64().and_then(|number| i32::try_from(number).ok())
}

/// The four money denominations, bounds-checked before extraction.
struct MoneyValue {
    pp: Option<i32>,
    gp: Option<i32>,
    sp: Option<i32>,
    cp: Option<i32>,
}

impl MoneyValue {
    fn from_json(value: &JsonValue) -> Self {
        let denomination = |key: &str| value.get(key).and_then(as_i32);
        Self {
            pp: denomination("pp"),
            gp: denomination("gp"),
            sp: denomination("sp"),
            cp: denomination("cp"),
        }
    }
}

#[cfg(test)]
mod bounds_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_bounds_table_rejects_and_accepts_exactly() {
        let hp = |value| validate_bounds(&vitals(VitalsField::Hp), &value);
        assert!(hp(json!(0)).is_ok(), "hp 0 is the floor");
        assert!(hp(json!(300)).is_ok());
        assert!(hp(json!(-1)).is_err(), "hp ≥ 0 is a bound");
        assert!(hp(json!("ten")).is_err(), "hp must be an integer");

        let level = |value| validate_bounds(&vitals(VitalsField::LevelAdjust), &value);
        assert!(level(json!(-19)).is_ok(), "level_adjust floor");
        assert!(level(json!(19)).is_ok(), "level_adjust ceiling");
        assert!(level(json!(25)).is_err(), "level_adjust ∈ −19..=19");

        let money = |value| validate_bounds(&vitals(VitalsField::Money), &value);
        assert!(money(json!({"pp": 1, "gp": 2, "sp": 3, "cp": 4})).is_ok());
        assert!(money(json!({"pp": -1, "gp": 2, "sp": 3, "cp": 4})).is_err());
        assert!(
            money(json!({"pp": 1, "gp": 2, "sp": 3})).is_err(),
            "all four"
        );

        // E6's spell-economy fields (design §3): the server validates the
        // shape and its own bounds only — the focus max clamp is the
        // character's, so it stays client-side (wire-protocol §3).
        let focus = |value| validate_bounds(&vitals(VitalsField::FocusCurrent), &value);
        assert!(focus(json!(0)).is_ok(), "focus 0 is the floor");
        assert!(focus(json!(3)).is_ok());
        assert!(focus(json!(-1)).is_err(), "focus >= 0 is a bound");
        assert!(focus(json!("two")).is_err(), "focus must be an integer");

        let hero = |value| validate_bounds(&vitals(VitalsField::HeroPoints), &value);
        assert!(hero(json!(0)).is_ok());
        assert!(hero(json!(-1)).is_err(), "hero points >= 0 is a bound");

        let daily = |value| validate_bounds(&vitals(VitalsField::Daily), &value);
        let zeroed = json!({"staff_charge_rank": 0, "staff_spent": 0, "drain_used": false});
        assert!(daily(zeroed).is_ok(), "the zeroed row is the shape");
        assert!(
            daily(json!({"staff_charge_rank": 10, "staff_spent": 2, "drain_used": true})).is_ok()
        );
        assert!(
            daily(json!({"staff_charge_rank": 11, "staff_spent": 0, "drain_used": false})).is_err(),
            "staff_charge_rank tops out at 10"
        );
        assert!(
            daily(json!({"staff_charge_rank": 0, "staff_spent": -1, "drain_used": false})).is_err(),
            "staff_spent >= 0 is a bound"
        );
        assert!(
            daily(json!({"staff_charge_rank": 0, "staff_spent": 0, "drain_used": "no"})).is_err(),
            "drain_used must be a boolean"
        );
        assert!(
            daily(json!({"staff_charge_rank": 0, "staff_spent": 0})).is_err(),
            "whole-row write: all three keys"
        );
        assert!(
            daily(
                json!({"staff_charge_rank": 0, "staff_spent": 0, "drain_used": false, "extra": 1})
            )
            .is_err(),
            "whole-row write: no extra keys"
        );
        assert!(daily(json!("zeroed")).is_err(), "daily must be an object");

        let slot = |value| {
            validate_bounds(
                &FieldTarget::Slot {
                    character_id: 1,
                    caster_key: "Wizard".to_owned(),
                    rank: 3,
                    slot_index: 0,
                },
                &value,
            )
        };
        assert!(slot(json!({"used": true})).is_ok());
        assert!(slot(json!({"used": true, "prepared": "Ray"})).is_ok());
        assert!(
            slot(json!({"used": "yes"})).is_err(),
            "used must be boolean"
        );
        assert!(slot(json!({"blast": 1})).is_err(), "unknown keys denied");

        let inv = |value| {
            validate_bounds(
                &FieldTarget::Inv {
                    character_id: 1,
                    item_name: "Chalk".to_owned(),
                },
                &value,
            )
        };
        assert!(inv(json!({"qty_delta": -2})).is_ok(), "signed deltas fine");
        assert!(inv(json!({"other": 1})).is_err());

        let effect = validate_bounds(
            &FieldTarget::Effect { effect_id: 7 },
            &json!({"active": false}),
        );
        assert!(effect.is_err(), "effect writes are E8's, refused here too");
    }

    fn vitals(field: VitalsField) -> FieldTarget {
        FieldTarget::Vitals {
            character_id: 1,
            field,
        }
    }
}
