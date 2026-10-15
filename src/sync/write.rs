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

use crate::auth::audit::{AuditEvent, AuditOutcome};
use crate::auth::authz::{Action, Actor, Resource, Role, Verdict, authorize};
use crate::engine_host::apply::apply_condition;
use crate::sync::protocol::{FieldTarget, Outcome, PartyField, VitalsField};

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
/// `reason` carries the rejection/forbidden text for the ack. `broadcast`
/// overrides the session's default diff payload — an effect op's diff is
/// the RESOLVED row (corpus-condition creates carry the applied signed
/// modifiers, and a create's address is the new id, not `effect_new`),
/// which only the write path knows.
#[derive(Debug)]
pub struct WriteResult {
    pub outcome: Outcome,
    pub version: Option<i64>,
    pub winning_version: Option<i64>,
    pub reason: Option<String>,
    /// The RESOLVED rows an applied write fans out, overriding the default
    /// echo of the op's own target and value — an effect op's diff is the
    /// resolved row (a create's address is the new id, not `effect_new`),
    /// and the E12 party writes fan the resolved row shape (D2). A mint or
    /// compound op carries SEVERAL diffs, each with its own version (a
    /// `stash_new` fans the stash row and its claim append, D7). Empty =
    /// the session's default echo; every entry names its own version.
    pub broadcast: Vec<(FieldTarget, JsonValue, i64)>,
    /// Characters whose derived numbers changed — old ∪ new targets of an
    /// applied effect op (sorted, deduped). Empty for field writes except
    /// `level_adjust` and `shield_raised`: both feed the engine's inputs
    /// (gh#59, gh#82), so an applied write fans out the recompute.
    pub affected: Vec<i64>,
}

impl WriteResult {
    /// A result whose diff is the op's own target and value (every plain
    /// field write; effect replays/supersessions and denials broadcast
    /// nothing). The session pairs the empty broadcast with the op's
    /// target, value, and applied version — the version arrives only after
    /// the CAS runs, so it cannot be filled here.
    fn echoing(_op: &ClientOp) -> Self {
        Self {
            outcome: Outcome::Applied,
            version: None,
            winning_version: None,
            reason: None,
            broadcast: Vec::new(),
            affected: Vec::new(),
        }
    }

    fn with_outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome;
        self
    }
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

    // 1b. Effect ops take their own path from here — their addressing,
    //     authz, and writes differ from a field CAS (create mints a row,
    //     retarget/end race on the whole-row version).
    if matches!(
        op.target,
        FieldTarget::Effect { .. } | FieldTarget::EffectNew { .. }
    ) {
        return apply_effect_write(pool, actor, party_id, op).await;
    }

    // 1c. Party-scoped targets (E12): the stash mint, the stash row, the
    //     bank, the quartermaster field. Their rows are the party's, not a
    //     character's, so resolution, authz (D4's fact-carrying variants),
    //     and apply all differ from the character-field CAS below.
    if matches!(
        op.target,
        FieldTarget::StashNew { .. }
            | FieldTarget::Stash { .. }
            | FieldTarget::Bank { .. }
            | FieldTarget::Party { .. }
    ) {
        return apply_party_write(pool, actor, party_id, op).await;
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

    let mut result = WriteResult::echoing(&op).with_outcome(outcome);
    result.version = version;
    result.winning_version = winning_version;
    if outcome != Outcome::Applied {
        // Only an applied write fans out; supersessions echo nothing.
        result.broadcast = Vec::new();
    }
    if outcome == Outcome::Applied
        && let FieldTarget::Vitals {
            character_id,
            field,
        } = op.target
        && matches!(field, VitalsField::LevelAdjust | VitalsField::ShieldRaised)
    {
        // eff_level feeds every proficiency-bearing stat, hp_max, and the
        // cantrip rank (E8 design D3's extraction table); the shield flag
        // gates the shield's AC bonus (gh#82) — a committed write to either
        // changes the engine's inputs, so the commit fans out the derived
        // consequence exactly like an effect write (D4/D6): one recompute
        // per affected character, after the diff, TCP-ordered.
        result.affected = vec![character_id];
    }
    Ok(result)
}

/// Why an effect op failed: a denial with a human-readable reason (bad
/// input, or E3's matrix), or a database failure that propagates.
enum EffectWriteError {
    Rejected(String),
    Forbidden(String),
    Database(anyhow::Error),
}

impl From<anyhow::Error> for EffectWriteError {
    fn from(err: anyhow::Error) -> Self {
        Self::Database(err)
    }
}

impl From<sqlx::Error> for EffectWriteError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(anyhow::Error::from(err))
    }
}

/// The effect write engine (E8, FR-7): create / retarget / end, one
/// transaction like every other op — scope → authz → ledger reservation →
/// write → ledger finalize. Authz is creator-only (E3's
/// [`Resource::Effect`]); the GM is denied everywhere; a corpus-sourced
/// create resolves its modifiers through `engine_host::apply` (D7) and
/// freezes them at apply time.
async fn apply_effect_write(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    op: ClientOp,
) -> anyhow::Result<WriteResult> {
    let op_name = op.value.get("op").and_then(JsonValue::as_str).unwrap_or("");
    let attempted = match (&op.target, op_name) {
        (FieldTarget::EffectNew { .. }, "create") => {
            apply_effect_create(pool, actor, party_id, &op).await
        }
        (FieldTarget::Effect { .. }, "update" | "end") => {
            apply_effect_mutation(pool, actor, party_id, &op).await
        }
        // Bounds already tied each op to its target kind; this arm keeps
        // the match honest for any future op name.
        _ => Err(EffectWriteError::Rejected(
            "effect op must be create (on effect_new), update, or end (on effect)".to_owned(),
        )),
    };
    match attempted {
        Ok(result) => Ok(result),
        Err(EffectWriteError::Rejected(reason)) => {
            record_denial(pool, actor, &op, Outcome::Rejected, reason).await
        }
        Err(EffectWriteError::Forbidden(reason)) => {
            record_denial(pool, actor, &op, Outcome::Forbidden, reason).await
        }
        Err(EffectWriteError::Database(err)) => Err(err),
    }
}

/// CREATE (FR-7/D7): validate, authorize (the writer owns the source
/// character, in this party), resolve corpus conditions, insert. CAS does
/// not apply — the identity PK mints the row; `base_version` is ignored.
async fn apply_effect_create(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    op: &ClientOp,
) -> Result<WriteResult, EffectWriteError> {
    let create = parse_create(&op.value).map_err(EffectWriteError::Rejected)?;
    check_create_scope(pool, actor, party_id, &create).await?;
    // Modifiers: corpus-sourced creates resolve through apply.rs (D7 — the
    // corpus data rules, the caller supplies only the condition value);
    // hand-built creates carry their own already-bounds-checked modifiers.
    let (modifiers, tracked_manually, corpus_entry_id) =
        resolve_create_modifiers(pool, &create).await?;

    // One transaction: ledger reservation → writes → ledger finalize.
    let mut tx = pool.begin().await.map_err(EffectWriteError::from)?;
    let request = request_json(op);
    let reserved = reserve_ledger(
        &mut tx,
        &op.op_id,
        &actor.sub,
        &field_path(&op.target),
        &request,
    )
    .await
    .map_err(EffectWriteError::from)?;
    if reserved.is_none() {
        tx.rollback().await.context("rollback id race")?;
        return answer_replay(pool, &op.op_id, &request)
            .await
            .map_err(EffectWriteError::from);
    }

    let (effect_id, version): (i64, i64) = sqlx::query_as(
        "INSERT INTO effects (party_id, source_character_id, name, duration_note, active, \
         version, tracked_manually, corpus_entry_id) \
         VALUES ($1, $2, $3, $4, true, nextval('field_version_seq'), $5, $6) \
         RETURNING id, version",
    )
    .bind(party_id)
    .bind(create.source_character_id)
    .bind(&create.name)
    .bind(&create.duration_note)
    .bind(tracked_manually)
    .bind(corpus_entry_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(EffectWriteError::from)?;
    for target_id in &create.targets {
        sqlx::query(
            "INSERT INTO effect_targets (party_id, effect_id, character_id) VALUES ($1, $2, $3)",
        )
        .bind(party_id)
        .bind(effect_id)
        .bind(target_id)
        .execute(&mut *tx)
        .await
        .map_err(EffectWriteError::from)?;
    }
    for (ord, modifier) in modifiers.iter().enumerate() {
        let ord = i32::try_from(ord).map_err(|source| {
            EffectWriteError::Rejected(format!("corpus row carries too many mappings ({source})"))
        })?;
        sqlx::query(
            "INSERT INTO effect_modifiers (effect_id, ord, type, stat, value) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(effect_id)
        .bind(ord)
        .bind(modifier.modifier_type.as_str())
        .bind(modifier.stat.as_str())
        .bind(modifier.value)
        .execute(&mut *tx)
        .await
        .map_err(EffectWriteError::from)?;
    }
    finalize_ledger(&mut tx, &op.op_id, Outcome::Applied, Some(version))
        .await
        .map_err(EffectWriteError::from)?;
    tx.commit().await.context("commit effect create")?;

    let value = effect_row_json(&create, &modifiers, tracked_manually);
    Ok(WriteResult {
        outcome: Outcome::Applied,
        version: Some(version),
        winning_version: None,
        reason: None,
        broadcast: vec![(FieldTarget::Effect { effect_id }, value, version)],
        affected: sorted_unique(&create.targets),
    })
}

/// UPDATE (retarget) and END, whole-row CAS on `effects.version` (FR-14):
/// creator-only, `superseded` on a lost race with the current version.
async fn apply_effect_mutation(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    op: &ClientOp,
) -> Result<WriteResult, EffectWriteError> {
    let FieldTarget::Effect { effect_id } = op.target else {
        return Err(EffectWriteError::Rejected(
            "update and end address an existing effect".to_owned(),
        ));
    };
    let is_end = op.value.get("op").and_then(JsonValue::as_str) == Some("end");
    authorize_effect_mutation(pool, actor, party_id, effect_id).await?;

    let mut tx = pool.begin().await.map_err(EffectWriteError::from)?;
    // The retarget's departing sheet must revert and the arriving one rise:
    // both sides of the swap are affected, so the old targets come off the
    // row BEFORE the CAS replaces them (end affects the same set — active
    // false drops the chip and the math).
    let old_targets = targets_of(&mut *tx, effect_id).await?;
    let request = request_json(op);
    let reserved = reserve_ledger(
        &mut tx,
        &op.op_id,
        &actor.sub,
        &field_path(&op.target),
        &request,
    )
    .await
    .map_err(EffectWriteError::from)?;
    if reserved.is_none() {
        tx.rollback().await.context("rollback id race")?;
        return answer_replay(pool, &op.op_id, &request)
            .await
            .map_err(EffectWriteError::from);
    }

    let outcome_write = if is_end {
        sqlx::query_scalar(
            "UPDATE effects SET active = false, \
             version = nextval('field_version_seq'), updated_at = now() \
             WHERE id = $1 AND version = $2 RETURNING version",
        )
        .bind(effect_id)
        .bind(op.base_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(EffectWriteError::from)?
    } else {
        let targets = parse_targets(&op.value).map_err(EffectWriteError::Rejected)?;
        for target_id in &targets {
            ensure_in_party_tx(&mut tx, party_id, *target_id).await?;
        }
        let updated: Option<i64> = sqlx::query_scalar(
            "UPDATE effects SET version = nextval('field_version_seq'), updated_at = now() \
             WHERE id = $1 AND version = $2 RETURNING version",
        )
        .bind(effect_id)
        .bind(op.base_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(EffectWriteError::from)?;
        if updated.is_some() {
            swap_targets(&mut tx, party_id, effect_id, &targets).await?;
        }
        updated
    };

    let Some(version) = outcome_write else {
        let winning: i64 = sqlx::query_scalar("SELECT version FROM effects WHERE id = $1")
            .bind(effect_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(EffectWriteError::from)?;
        finalize_ledger(&mut tx, &op.op_id, Outcome::Superseded, None)
            .await
            .map_err(EffectWriteError::from)?;
        tx.commit().await.context("commit supersession")?;
        return Ok(WriteResult {
            outcome: Outcome::Superseded,
            version: None,
            winning_version: Some(winning),
            reason: None,
            broadcast: Vec::new(),
            affected: Vec::new(),
        });
    };

    finalize_ledger(&mut tx, &op.op_id, Outcome::Applied, Some(version))
        .await
        .map_err(EffectWriteError::from)?;
    tx.commit().await.context("commit effect mutation")?;

    // The diff value is the resolved whole row, snapshot shape — the
    // retargeted target set or the ended state, read back committed.
    let value = read_effect_row(pool, effect_id)
        .await
        .map_err(EffectWriteError::from)?;
    let new_targets = targets_of(pool, effect_id).await?;
    Ok(WriteResult {
        outcome: Outcome::Applied,
        version: Some(version),
        winning_version: None,
        reason: None,
        broadcast: vec![(FieldTarget::Effect { effect_id }, value, version)],
        affected: sorted_unique(old_targets.iter().chain(&new_targets)),
    })
}

/// The authorization half of update/end: the effect must sit in this
/// party, and the writer must be its creator (GM denied, E3 verbatim).
/// Returns the op kind (`true` = end) so the caller branches once.
async fn authorize_effect_mutation(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    effect_id: i64,
) -> Result<(), EffectWriteError> {
    let row: Option<(i64, i64)> =
        sqlx::query_as("SELECT party_id, source_character_id FROM effects WHERE id = $1")
            .bind(effect_id)
            .fetch_optional(pool)
            .await
            .map_err(EffectWriteError::from)?;
    let Some((effect_party, source_character_id)) = row else {
        return Err(EffectWriteError::Rejected("unknown target".to_owned()));
    };
    if effect_party != party_id {
        return Err(EffectWriteError::Forbidden(
            "target effect is not in this party".to_owned(),
        ));
    }
    let creator_sub: String = sqlx::query_scalar("SELECT owner_sub FROM characters WHERE id = $1")
        .bind(source_character_id)
        .fetch_one(pool)
        .await
        .map_err(EffectWriteError::from)?;
    if authorize(
        actor,
        Action::Write,
        &Resource::Effect {
            creator_sub: creator_sub.clone(),
        },
    ) == Verdict::Deny
    {
        let reason = if actor.role == Role::Gm {
            "gm is read-only".to_owned()
        } else {
            "only the effect's creator may change it".to_owned()
        };
        return Err(EffectWriteError::Forbidden(reason));
    }
    Ok(())
}

/// Transaction-side roster check (same rule as [`ensure_in_party`]).
async fn ensure_in_party_tx(
    tx: &mut Transaction<'_, Postgres>,
    party_id: i64,
    target_id: i64,
) -> Result<(), EffectWriteError> {
    let in_party: Option<i64> =
        sqlx::query_scalar("SELECT id FROM characters WHERE id = $1 AND party_id = $2")
            .bind(target_id)
            .bind(party_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(EffectWriteError::from)?;
    if in_party.is_none() {
        return Err(EffectWriteError::Rejected(format!(
            "target {target_id} is not a roster character of this party"
        )));
    }
    Ok(())
}

/// Replace the effect's whole target set (inside the CAS win).
async fn swap_targets(
    tx: &mut Transaction<'_, Postgres>,
    party_id: i64,
    effect_id: i64,
    targets: &[i64],
) -> Result<(), EffectWriteError> {
    sqlx::query("DELETE FROM effect_targets WHERE effect_id = $1")
        .bind(effect_id)
        .execute(&mut **tx)
        .await
        .map_err(EffectWriteError::from)?;
    for target_id in targets {
        sqlx::query(
            "INSERT INTO effect_targets (party_id, effect_id, character_id) VALUES ($1, $2, $3)",
        )
        .bind(party_id)
        .bind(effect_id)
        .bind(target_id)
        .execute(&mut **tx)
        .await
        .map_err(EffectWriteError::from)?;
    }
    Ok(())
}

/// A row's target set, id order (both reads of the retarget delta).
async fn targets_of(
    executor: impl sqlx::PgExecutor<'_>,
    effect_id: i64,
) -> Result<Vec<i64>, EffectWriteError> {
    sqlx::query_scalar(
        "SELECT character_id FROM effect_targets WHERE effect_id = $1 ORDER BY character_id",
    )
    .bind(effect_id)
    .fetch_all(executor)
    .await
    .map_err(EffectWriteError::from)
}

/// Sorted, deduped — the fan-out set for one effect op.
fn sorted_unique<'a>(ids: impl IntoIterator<Item = &'a i64>) -> Vec<i64> {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        seen.insert(*id);
    }
    seen.into_iter().collect()
}

/// The committed effect row in the snapshot/diff value shape.
async fn read_effect_row(pool: &PgPool, effect_id: i64) -> anyhow::Result<JsonValue> {
    let (name, source_character_id, duration_note, active, tracked_manually): (
        String,
        i64,
        String,
        bool,
        bool,
    ) = sqlx::query_as(
        "SELECT name, source_character_id, duration_note, active, tracked_manually \
         FROM effects WHERE id = $1",
    )
    .bind(effect_id)
    .fetch_one(pool)
    .await
    .context("read effect row for diff")?;
    let targets: Vec<i64> = sqlx::query_scalar(
        "SELECT character_id FROM effect_targets WHERE effect_id = $1 ORDER BY character_id",
    )
    .bind(effect_id)
    .fetch_all(pool)
    .await
    .context("read effect targets for diff")?;
    let modifiers: Vec<(String, String, i32)> = sqlx::query_as(
        "SELECT type, stat, value FROM effect_modifiers WHERE effect_id = $1 ORDER BY ord",
    )
    .bind(effect_id)
    .fetch_all(pool)
    .await
    .context("read effect modifiers for diff")?;
    let modifiers_json: Vec<JsonValue> = modifiers
        .into_iter()
        .map(|(modifier_type, stat, value)| {
            serde_json::json!({"type": modifier_type, "stat": stat, "value": value})
        })
        .collect();
    Ok(serde_json::json!({
        "name": name,
        "source_character_id": source_character_id,
        "targets": targets,
        "modifiers": modifiers_json,
        "duration_note": duration_note,
        "active": active,
        "tracked_manually": tracked_manually,
    }))
}

/// The create payload's diff value: the same shape, built from what was
/// just written (nothing to re-read).
fn effect_row_json(
    create: &EffectCreate,
    modifiers: &[hireling_engine::model::Modifier],
    tracked_manually: bool,
) -> JsonValue {
    let modifiers_json: Vec<JsonValue> = modifiers
        .iter()
        .map(|modifier| {
            serde_json::json!({
                "type": modifier.modifier_type.as_str(),
                "stat": modifier.stat.as_str(),
                "value": modifier.value,
            })
        })
        .collect();
    serde_json::json!({
        "name": create.name,
        "source_character_id": create.source_character_id,
        "targets": create.targets,
        "modifiers": modifiers_json,
        "duration_note": create.duration_note,
        "active": true,
        "tracked_manually": tracked_manually,
    })
}

/// The ownership and party-scope facts for a create: the source character
/// exists inside THIS party and is owned by the writer (FR-10), and every
/// target is roster (PRD FG3).
async fn check_create_scope(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    create: &EffectCreate,
) -> Result<(), EffectWriteError> {
    let source: Option<(String, i64)> =
        sqlx::query_as("SELECT owner_sub, party_id FROM characters WHERE id = $1")
            .bind(create.source_character_id)
            .fetch_optional(pool)
            .await
            .map_err(EffectWriteError::from)?;
    let Some((owner_sub, source_party)) = source else {
        return Err(EffectWriteError::Rejected(
            "source_character_id does not exist".to_owned(),
        ));
    };
    if source_party != party_id {
        return Err(EffectWriteError::Rejected(
            "source_character_id is not in this party".to_owned(),
        ));
    }
    // The GM's denial names the matrix, not the ownership fact (E3's
    // reasons, verbatim) — so the GM check runs first.
    if actor.role == Role::Gm {
        return Err(EffectWriteError::Forbidden("gm is read-only".to_owned()));
    }
    if authorize(actor, Action::Write, &Resource::Character { owner_sub }) == Verdict::Deny {
        return Err(EffectWriteError::Rejected(
            "you do not own the source character".to_owned(),
        ));
    }
    for target_id in &create.targets {
        ensure_in_party(pool, party_id, *target_id).await?;
    }
    Ok(())
}

/// Every target must be a roster character of the effect's own party.
async fn ensure_in_party(
    pool: &PgPool,
    party_id: i64,
    target_id: i64,
) -> Result<(), EffectWriteError> {
    let in_party: Option<i64> =
        sqlx::query_scalar("SELECT id FROM characters WHERE id = $1 AND party_id = $2")
            .bind(target_id)
            .bind(party_id)
            .fetch_optional(pool)
            .await
            .map_err(EffectWriteError::from)?;
    if in_party.is_none() {
        return Err(EffectWriteError::Rejected(format!(
            "target {target_id} is not a roster character of this party"
        )));
    }
    Ok(())
}

/// What one party apply produced inside its transaction: the outcome triple
/// plus the resolved-row diffs to fan out on commit (D2's shapes; a mint
/// carries its row AND its claim append, D7). A `Rejected` here is
/// mid-transaction — the caller rolls back and records the denial durably.
struct PartyApply {
    outcome: Outcome,
    version: Option<i64>,
    winning_version: Option<i64>,
    reason: Option<String>,
    diffs: Vec<(FieldTarget, JsonValue, i64)>,
}

impl PartyApply {
    fn rejected(reason: String) -> Self {
        Self {
            outcome: Outcome::Rejected,
            version: None,
            winning_version: None,
            reason: Some(reason),
            diffs: Vec::new(),
        }
    }

    fn superseded(winning_version: i64) -> Self {
        Self {
            outcome: Outcome::Superseded,
            version: None,
            winning_version: Some(winning_version),
            reason: None,
            diffs: Vec::new(),
        }
    }
}

/// The party-scoped write engine (E12, design D2): the stash mint, the
/// additive stash-qty/bank applies, the CAS meta/quartermaster writes — one
/// transaction like every other op: party scope → authz → ledger
/// reservation → write → ledger finalize. The GM is denied first by the
/// role check (D4; audits as `forbidden_gm_write`); the matrix reads the
/// D4 fact-carrying variants with facts loaded fresh per message. Additive
/// applies ignore `base_version` (FR-2.5: the frame carries it, the apply
/// ignores it) and are never `superseded`; CAS remains for the absolute
/// fields (meta, the designation). Denials audit per FR-10 under the D1.4
/// values.
async fn apply_party_write(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    op: ClientOp,
) -> anyhow::Result<WriteResult> {
    if let Some(denial) = party_write_scope_denial(pool, actor, party_id, &op).await? {
        return Ok(denial);
    }
    if let Some(denial) = party_write_matrix_denial(pool, actor, party_id, &op).await? {
        return Ok(denial);
    }

    // One transaction: ledger reservation → write → ledger finalize (the
    // character-path discipline; FR-9 for the mint's claim row).
    let mut tx = pool
        .begin()
        .await
        .context("begin party write transaction")?;
    let request = request_json(&op);
    let reserved = reserve_ledger(
        &mut tx,
        &op.op_id,
        &actor.sub,
        &field_path(&op.target),
        &request,
    )
    .await?;
    if reserved.is_none() {
        tx.rollback().await.context("rollback id race")?;
        return answer_replay(pool, &op.op_id, &request).await;
    }

    let mut applied = match &op.target {
        FieldTarget::StashNew {
            party_id: target_party_id,
        } => stash_new_tx(&mut tx, *target_party_id, actor, &op.value).await,
        FieldTarget::Stash {
            party_id: target_party_id,
            item_name,
        } => {
            stash_tx(
                &mut tx,
                *target_party_id,
                item_name,
                op.base_version,
                &op.value,
            )
            .await
        }
        FieldTarget::Bank {
            party_id: target_party_id,
        } => bank_tx(&mut tx, *target_party_id, &op.value).await,
        FieldTarget::Party {
            party_id: target_party_id,
            field: PartyField::Quartermaster,
        } => quartermaster_tx(&mut tx, *target_party_id, op.base_version, &op.value).await,
        // An off-route target: refused, nothing written. (A future
        // PartyField with no wired arm makes this match non-exhaustive —
        // the compiler will name the arm to add.)
        FieldTarget::Claim { .. }
        | FieldTarget::Vitals { .. }
        | FieldTarget::Slot { .. }
        | FieldTarget::Inv { .. }
        | FieldTarget::Effect { .. }
        | FieldTarget::EffectNew { .. } => Ok(PartyApply::rejected("unknown target".to_owned())),
    }?;
    if applied.outcome == Outcome::Rejected {
        // Nothing partial: the transaction (mint row, claim row, minted
        // bank row — whatever the arm touched) rolls back whole, and the
        // refusal is recorded durably by the denial path.
        tx.rollback()
            .await
            .context("rollback refused party write")?;
        let reason = applied
            .reason
            .unwrap_or_else(|| "unknown target".to_owned());
        return record_denial(pool, actor, &op, Outcome::Rejected, reason).await;
    }
    finalize_ledger(&mut tx, &op.op_id, applied.outcome, applied.version).await?;
    tx.commit().await.context("commit party write")?;

    let mut result = WriteResult::echoing(&op).with_outcome(applied.outcome);
    result.version = applied.version;
    result.winning_version = applied.winning_version;
    result.reason = applied.reason.take();
    result.broadcast = std::mem::take(&mut applied.diffs);
    Ok(result)
}

/// Party scope + GM gate (D4). The socket's party is the write's addressing
/// scope (contract §1) — a target naming another party is forbidden even
/// when the writer belongs to it. Returns the denial to short-circuit, or
/// `None` to proceed.
async fn party_write_scope_denial(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    op: &ClientOp,
) -> anyhow::Result<Option<WriteResult>> {
    let target_party = match &op.target {
        FieldTarget::StashNew {
            party_id: target_party_id,
        }
        | FieldTarget::Stash {
            party_id: target_party_id,
            ..
        }
        | FieldTarget::Bank {
            party_id: target_party_id,
        }
        | FieldTarget::Party {
            party_id: target_party_id,
            ..
        } => *target_party_id,
        // Bounds rejects claim writes and the route admits only the four
        // party targets; these arms keep the match honest if either gate
        // ever moves.
        FieldTarget::Claim { .. }
        | FieldTarget::Vitals { .. }
        | FieldTarget::Slot { .. }
        | FieldTarget::Inv { .. }
        | FieldTarget::Effect { .. }
        | FieldTarget::EffectNew { .. } => {
            return record_denial(
                pool,
                actor,
                op,
                Outcome::Rejected,
                "unknown target".to_owned(),
            )
            .await
            .map(Some);
        }
    };
    if target_party != party_id {
        return record_denial(
            pool,
            actor,
            op,
            Outcome::Forbidden,
            "target party is not this party".to_owned(),
        )
        .await
        .map(Some);
    }
    // The GM gate: named and audited as the role denial (D4; Task 3's
    // acceptance pins the value).
    if actor.role == Role::Gm {
        return record_party_denial(
            pool,
            actor,
            op,
            "gm is read-only".to_owned(),
            AuditEvent::ForbiddenGmWrite,
        )
        .await
        .map(Some);
    }
    Ok(None)
}

/// E3's matrix per message, the deciding fact loaded fresh (D4). Each
/// surface audits under its own D1.4 value on a denial.
async fn party_write_matrix_denial(
    pool: &PgPool,
    actor: &Actor,
    party_id: i64,
    op: &ClientOp,
) -> anyhow::Result<Option<WriteResult>> {
    match &op.target {
        FieldTarget::StashNew { .. } | FieldTarget::Stash { .. } => {
            let member = is_party_member(pool, actor, party_id).await?;
            if authorize(
                actor,
                Action::Write,
                &Resource::Stash {
                    actor_is_party_member: member,
                },
            ) == Verdict::Deny
            {
                return record_party_denial(
                    pool,
                    actor,
                    op,
                    "not a member of this party".to_owned(),
                    AuditEvent::ForbiddenStashWrite,
                )
                .await
                .map(Some);
            }
        }
        FieldTarget::Bank { .. } => {
            let owner = quartermaster_owner(pool, party_id).await?;
            if authorize(
                actor,
                Action::Write,
                &Resource::Bank {
                    quartermaster_owner_sub: owner.clone(),
                },
            ) == Verdict::Deny
            {
                let reason = if owner.is_none() {
                    "no quartermaster is designated"
                } else {
                    "only the quartermaster's owner may move the bank"
                };
                return record_party_denial(
                    pool,
                    actor,
                    op,
                    reason.to_owned(),
                    AuditEvent::ForbiddenBankWrite,
                )
                .await
                .map(Some);
            }
        }
        FieldTarget::Party { .. } => {
            let member = is_party_member(pool, actor, party_id).await?;
            if authorize(
                actor,
                Action::Write,
                &Resource::PartySettings {
                    actor_is_party_member: member,
                },
            ) == Verdict::Deny
            {
                return record_party_denial(
                    pool,
                    actor,
                    op,
                    "not a member of this party".to_owned(),
                    AuditEvent::ForbiddenPartyWrite,
                )
                .await
                .map(Some);
            }
        }
        // Covered by the scope match above.
        FieldTarget::Claim { .. }
        | FieldTarget::Vitals { .. }
        | FieldTarget::Slot { .. }
        | FieldTarget::Inv { .. }
        | FieldTarget::Effect { .. }
        | FieldTarget::EffectNew { .. } => {
            return record_denial(
                pool,
                actor,
                op,
                Outcome::Rejected,
                "unknown target".to_owned(),
            )
            .await
            .map(Some);
        }
    }
    Ok(None)
}

/// Record a party-write denial durably AND audit it (FR-10: every refusal
/// in the audit trail under the D1.4 values, with the actor's identity).
/// The ledger row is the verdict of record; the audit row is best-effort —
/// the verdict is already delivered and must not change.
async fn record_party_denial(
    pool: &PgPool,
    actor: &Actor,
    op: &ClientOp,
    reason: String,
    event: AuditEvent,
) -> anyhow::Result<WriteResult> {
    let result = record_denial(pool, actor, op, Outcome::Forbidden, reason).await?;
    crate::auth::audit::record(
        pool,
        event,
        Some(&actor.sub),
        &field_path(&op.target),
        AuditOutcome::Denied,
        None,
    )
    .await;
    Ok(result)
}

/// The membership fact, loaded fresh per message (D4: facts are inputs).
async fn is_party_member(pool: &PgPool, actor: &Actor, party_id: i64) -> anyhow::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM characters WHERE party_id = $1 AND owner_sub = $2)",
    )
    .bind(party_id)
    .bind(&actor.sub)
    .fetch_one(pool)
    .await
    .context("party membership lookup")
}

/// The `sub` of the account owning the designated quartermaster character,
/// or `None` when the party has none (FR-5's undesignated case).
async fn quartermaster_owner(pool: &PgPool, party_id: i64) -> anyhow::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.owner_sub FROM parties p \
         LEFT JOIN characters c ON c.id = p.quartermaster_character_id \
         WHERE p.id = $1",
    )
    .bind(party_id)
    .fetch_one(pool)
    .await
    .context("quartermaster owner lookup")
}

/// The stash mint (D2): insert the row; a racing duplicate name hits the
/// D1.5 unique constraint and maps to FR-1's friendly refusal; the claim
/// row (`outside → stash`) lands in the same transaction (FR-9). The diff
/// is the resolved row plus the claim append (D7).
async fn stash_new_tx(
    tx: &mut Transaction<'_, Postgres>,
    party_id: i64,
    actor: &Actor,
    value: &JsonValue,
) -> anyhow::Result<PartyApply> {
    let item_name = value
        .get("item_name")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let quantity = as_i32(value.get("quantity").unwrap_or(&JsonValue::Null));
    let bulk = stash_bulk_of(value.get("bulk"));
    let notes = value
        .get("notes")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .to_owned();
    let minted: Result<(i32, Option<f64>, String, i64), sqlx::Error> = sqlx::query_as(
        "INSERT INTO party_stash (party_id, item_name, quantity, bulk, notes) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING quantity, bulk::float8 AS bulk, notes, version",
    )
    .bind(party_id)
    .bind(&item_name)
    .bind(quantity)
    .bind(bulk)
    .bind(&notes)
    .fetch_one(&mut **tx)
    .await;
    let (minted_quantity, minted_bulk, minted_notes, row_version) = match minted {
        Ok(row) => row,
        Err(err) if is_stash_name_violation(&err) => {
            // FR-1's friendly refusal: the unique constraint is the
            // structural identity; this message is its mapped surface.
            return Ok(PartyApply::rejected(format!(
                "\"{item_name}\" is already in the stash — adjust that row's quantity instead of adding it again"
            )));
        }
        Err(err) => return Err(anyhow::Error::from(err).context("stash mint")),
    };
    let claim_id: i64 = sqlx::query_scalar(
        "INSERT INTO claim_history (party_id, item_name, quantity, origin, destination, actor_sub) \
         VALUES ($1, $2, $3, 'outside', 'stash', $4) RETURNING id",
    )
    .bind(party_id)
    .bind(&item_name)
    .bind(minted_quantity)
    .bind(&actor.sub)
    .fetch_one(&mut **tx)
    .await
    .context("stash mint claim row")?;
    let claim_value = read_claim_row(&mut **tx, claim_id).await?;
    Ok(PartyApply {
        outcome: Outcome::Applied,
        version: Some(row_version),
        winning_version: None,
        reason: None,
        diffs: vec![
            (
                FieldTarget::Stash {
                    party_id,
                    item_name: item_name.clone(),
                },
                serde_json::json!({
                    "item_name": item_name,
                    "quantity": minted_quantity,
                    "bulk": minted_bulk,
                    "notes": minted_notes,
                }),
                row_version,
            ),
            (FieldTarget::Claim { claim_id }, claim_value, claim_id),
        ],
    })
}

/// The stash row's apply: `qty` additive (underflow/unknown-row refusal,
/// never superseded) or `meta` CAS on the row version (D2). The diff is the
/// resolved row shape in both cases.
async fn stash_tx(
    tx: &mut Transaction<'_, Postgres>,
    party_id: i64,
    item_name: &str,
    base_version: i64,
    value: &JsonValue,
) -> anyhow::Result<PartyApply> {
    let op = value.get("op").and_then(JsonValue::as_str).unwrap_or("");
    match op {
        "qty" => stash_qty_tx(tx, party_id, item_name, value).await,
        "meta" => stash_meta_tx(tx, party_id, item_name, base_version, value).await,
        // Bounds admits only qty and meta (Task 3 adds transfer/sell).
        _ => Ok(PartyApply::rejected(format!(
            "unknown stash op {op:?} (qty and meta are the wired ops)"
        ))),
    }
}

/// The additive qty arm (D2): no version predicate — a stale base still
/// applies, serialized by the row lock. An impossible draw refuses with a
/// pre-formatted reason instead of erroring the driver.
async fn stash_qty_tx(
    tx: &mut Transaction<'_, Postgres>,
    party_id: i64,
    item_name: &str,
    value: &JsonValue,
) -> anyhow::Result<PartyApply> {
    let qty_delta = as_i32(value.get("qty_delta").unwrap_or(&JsonValue::Null));
    // The quantity guard carries the shipped CHECK's semantics (> 0) plus
    // the column's ceiling, so an impossible apply refuses instead of
    // erroring the driver.
    let updated: Option<(i32, Option<f64>, String, i64)> = sqlx::query_as(
        "UPDATE party_stash SET quantity = quantity + $1, \
         version = nextval('field_version_seq'), updated_at = now() \
         WHERE party_id = $2 AND item_name = $3 \
           AND quantity + $1 > 0 AND quantity + $1 <= $4 \
         RETURNING quantity, bulk::float8 AS bulk, notes, version",
    )
    .bind(qty_delta)
    .bind(party_id)
    .bind(item_name)
    .bind(i32::MAX)
    .fetch_optional(&mut **tx)
    .await
    .context("additive stash qty")?;
    let Some((quantity, bulk, notes, version)) = updated else {
        // Missing row vs refused draw: read the row to tell them apart
        // (contract §4: unknown target is `rejected`).
        let existing: Option<i32> = sqlx::query_scalar(
            "SELECT quantity FROM party_stash WHERE party_id = $1 AND item_name = $2",
        )
        .bind(party_id)
        .bind(item_name)
        .fetch_optional(&mut **tx)
        .await
        .context("stash row read")?;
        return Ok(match existing {
            None => PartyApply::rejected("unknown target".to_owned()),
            Some(current) => {
                let target = i64::from(current) + i64::from(qty_delta.unwrap_or_default());
                if target <= 0 {
                    PartyApply::rejected(format!(
                        "that draw would drive \"{item_name}\" below zero \
                         ({current} in the stash, {delta} asked)",
                        delta = qty_delta.unwrap_or_default()
                    ))
                } else {
                    PartyApply::rejected(format!(
                        "that draw would overflow \"{item_name}\"'s quantity \
                         ({current} in the stash, {delta} asked)",
                        delta = qty_delta.unwrap_or_default()
                    ))
                }
            }
        });
    };
    Ok(PartyApply {
        outcome: Outcome::Applied,
        version: Some(version),
        winning_version: None,
        reason: None,
        diffs: vec![(
            FieldTarget::Stash {
                party_id,
                item_name: item_name.to_owned(),
            },
            serde_json::json!({
                "item_name": item_name,
                "quantity": quantity,
                "bulk": bulk,
                "notes": notes,
            }),
            version,
        )],
    })
}

/// The CAS meta arm (D2, FR-2.2): the provided keys are set, the row
/// version must still be base. A stale base supersedes; a missing row is
/// rejected.
async fn stash_meta_tx(
    tx: &mut Transaction<'_, Postgres>,
    party_id: i64,
    item_name: &str,
    base_version: i64,
    value: &JsonValue,
) -> anyhow::Result<PartyApply> {
    let set_bulk = value.get("bulk").is_some();
    let set_notes = value.get("notes").is_some();
    let bulk = stash_bulk_of(value.get("bulk"));
    let notes = value.get("notes").and_then(JsonValue::as_str);
    let updated: Option<(i32, Option<f64>, String, i64)> = if set_bulk && set_notes {
        sqlx::query_as(
            "UPDATE party_stash SET bulk = $1, notes = $2, \
             version = nextval('field_version_seq'), updated_at = now() \
             WHERE party_id = $3 AND item_name = $4 AND version = $5 \
             RETURNING quantity, bulk::float8 AS bulk, notes, version",
        )
        .bind(bulk)
        .bind(notes)
        .bind(party_id)
        .bind(item_name)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS stash meta (bulk+notes)")?
    } else if set_bulk {
        sqlx::query_as(
            "UPDATE party_stash SET bulk = $1, \
             version = nextval('field_version_seq'), updated_at = now() \
             WHERE party_id = $2 AND item_name = $3 AND version = $4 \
             RETURNING quantity, bulk::float8 AS bulk, notes, version",
        )
        .bind(bulk)
        .bind(party_id)
        .bind(item_name)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS stash meta (bulk)")?
    } else if set_notes {
        sqlx::query_as(
            "UPDATE party_stash SET notes = $1, \
             version = nextval('field_version_seq'), updated_at = now() \
             WHERE party_id = $2 AND item_name = $3 AND version = $4 \
             RETURNING quantity, bulk::float8 AS bulk, notes, version",
        )
        .bind(notes)
        .bind(party_id)
        .bind(item_name)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS stash meta (notes)")?
    } else {
        // Bounds requires at least one field; this arm keeps the branch
        // total if that gate ever moves.
        return Ok(PartyApply::rejected(
            "stash meta needs bulk or notes".to_owned(),
        ));
    };
    let Some((quantity, resolved_bulk, resolved_notes, version)) = updated else {
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM party_stash WHERE party_id = $1 AND item_name = $2",
        )
        .bind(party_id)
        .bind(item_name)
        .fetch_optional(&mut **tx)
        .await
        .context("stash row version read")?;
        return Ok(match existing {
            None => PartyApply::rejected("unknown target".to_owned()),
            Some(winning) => PartyApply::superseded(winning),
        });
    };
    Ok(PartyApply {
        outcome: Outcome::Applied,
        version: Some(version),
        winning_version: None,
        reason: None,
        diffs: vec![(
            FieldTarget::Stash {
                party_id,
                item_name: item_name.to_owned(),
            },
            serde_json::json!({
                "item_name": item_name,
                "quantity": quantity,
                "bulk": resolved_bulk,
                "notes": resolved_notes,
            }),
            version,
        )],
    })
}

/// The bank's additive apply (D2): one version unit, signed per-denomination
/// deltas, the shipped CHECKs' semantics enforced in the predicate so an
/// impossible apply refuses with a pre-formatted reason instead of erroring
/// the driver. The balance row mints with the party's first movement.
async fn bank_tx(
    tx: &mut Transaction<'_, Postgres>,
    party_id: i64,
    value: &JsonValue,
) -> anyhow::Result<PartyApply> {
    let gp_delta = as_i32(value.get("gp_delta").unwrap_or(&JsonValue::Null));
    let sp_delta = as_i32(value.get("sp_delta").unwrap_or(&JsonValue::Null));
    let cp_delta = as_i32(value.get("cp_delta").unwrap_or(&JsonValue::Null));
    sqlx::query("INSERT INTO party_bank (party_id) VALUES ($1) ON CONFLICT (party_id) DO NOTHING")
        .bind(party_id)
        .execute(&mut **tx)
        .await
        .context("ensure bank row")?;
    // Additive: no version predicate — a stale base still credits, serialized
    // by the row lock (FR-2.1).
    let updated: Option<(i32, i32, i32, i64)> = sqlx::query_as(
        "UPDATE party_bank SET gp = gp + $1, sp = sp + $2, cp = cp + $3, \
         version = nextval('field_version_seq'), updated_at = now() \
         WHERE party_id = $4 \
           AND gp + $1 >= 0 AND sp + $2 >= 0 AND cp + $3 >= 0 \
           AND gp + $1 <= $5 AND sp + $2 <= $5 AND cp + $3 <= $5 \
         RETURNING gp, sp, cp, version",
    )
    .bind(gp_delta)
    .bind(sp_delta)
    .bind(cp_delta)
    .bind(party_id)
    .bind(i32::MAX)
    .fetch_optional(&mut **tx)
    .await
    .context("additive bank apply")?;
    let Some((gp, sp, cp, version)) = updated else {
        let (gp, sp, cp): (i32, i32, i32) =
            sqlx::query_as("SELECT gp, sp, cp FROM party_bank WHERE party_id = $1")
                .bind(party_id)
                .fetch_one(&mut **tx)
                .await
                .context("bank row read")?;
        let denominations = [
            ("gp", gp, gp_delta.unwrap_or_default()),
            ("sp", sp, sp_delta.unwrap_or_default()),
            ("cp", cp, cp_delta.unwrap_or_default()),
        ];
        let reason = match denominations
            .iter()
            .find(|(_name, current, delta)| i64::from(*current) + i64::from(*delta) < 0)
        {
            Some((name, current, delta)) => format!(
                "that adjustment would drive {name} below zero ({current} on the row, {delta} asked)"
            ),
            None => "that adjustment would overflow the balance".to_owned(),
        };
        return Ok(PartyApply::rejected(reason));
    };
    Ok(PartyApply {
        outcome: Outcome::Applied,
        version: Some(version),
        winning_version: None,
        reason: None,
        diffs: vec![(
            FieldTarget::Bank { party_id },
            serde_json::json!({"gp": gp, "sp": sp, "cp": cp}),
            version,
        )],
    })
}

/// The quartermaster designation: an absolute write under CAS on
/// `quartermaster_version` (FR-2.3/FR-5); the value names a roster character
/// or clears the designation.
async fn quartermaster_tx(
    tx: &mut Transaction<'_, Postgres>,
    party_id: i64,
    base_version: i64,
    value: &JsonValue,
) -> anyhow::Result<PartyApply> {
    let character_id = value.get("character_id").and_then(JsonValue::as_i64);
    if let Some(character_id) = character_id {
        let in_party: Option<i64> =
            sqlx::query_scalar("SELECT id FROM characters WHERE id = $1 AND party_id = $2")
                .bind(character_id)
                .bind(party_id)
                .fetch_optional(&mut **tx)
                .await
                .context("roster check")?;
        if in_party.is_none() {
            return Ok(PartyApply::rejected(format!(
                "character {character_id} is not a roster character of this party"
            )));
        }
    }
    let new_version: Option<i64> = sqlx::query_scalar(
        "UPDATE parties SET quartermaster_character_id = $1, \
         quartermaster_version = nextval('field_version_seq'), updated_at = now() \
         WHERE id = $2 AND quartermaster_version = $3 \
         RETURNING quartermaster_version",
    )
    .bind(character_id)
    .bind(party_id)
    .bind(base_version)
    .fetch_optional(&mut **tx)
    .await
    .context("CAS quartermaster")?;
    let Some(version) = new_version else {
        let winning: i64 =
            sqlx::query_scalar("SELECT quartermaster_version FROM parties WHERE id = $1")
                .bind(party_id)
                .fetch_one(&mut **tx)
                .await
                .context("re-read quartermaster version")?;
        return Ok(PartyApply::superseded(winning));
    };
    Ok(PartyApply {
        outcome: Outcome::Applied,
        version: Some(version),
        winning_version: None,
        reason: None,
        diffs: vec![(
            FieldTarget::Party {
                party_id,
                field: PartyField::Quartermaster,
            },
            serde_json::json!({"quartermaster_character_id": character_id}),
            version,
        )],
    })
}

/// One committed claim row (D7). Coins are NULL on item rows (outside ↔
/// stash); they are set only on the minted bank rows (Task 5). Decoded
/// as a struct so the row shape has one owner instead of a nine-tuple
/// repeated in the snapshot path.
#[derive(sqlx::FromRow)]
struct ClaimRow {
    item_name: String,
    quantity: i32,
    origin: String,
    destination: String,
    actor_sub: String,
    created_at: chrono::DateTime<chrono::Utc>,
    coins_gp: Option<i32>,
    coins_sp: Option<i32>,
    coins_cp: Option<i32>,
}

/// One committed claim row in the snapshot/diff value shape (D7): the id
/// rides the target, the fields ride the value, coins NULL on item rows.
pub(crate) async fn read_claim_row(
    executor: impl sqlx::PgExecutor<'_>,
    claim_id: i64,
) -> anyhow::Result<JsonValue> {
    let row = sqlx::query_as::<_, ClaimRow>(
        "SELECT item_name, quantity, origin, destination, actor_sub, created_at, \
         coins_gp, coins_sp, coins_cp FROM claim_history WHERE id = $1",
    )
    .bind(claim_id)
    .fetch_one(executor)
    .await
    .context("read claim row for diff")?;
    Ok(serde_json::json!({
        "item_name": row.item_name,
        "quantity": row.quantity,
        "origin": row.origin,
        "destination": row.destination,
        "actor_sub": row.actor_sub,
        "created_at": row.created_at,
        "coins_gp": row.coins_gp,
        "coins_sp": row.coins_sp,
        "coins_cp": row.coins_cp,
    }))
}

/// `party_stash.bulk` extraction — bounds validated the range; absent or
/// null reads as NULL (the schema's negligible/unknown convention).
fn stash_bulk_of(value: Option<&JsonValue>) -> Option<f64> {
    value.and_then(JsonValue::as_f64)
}

/// The D1.5 unique violation behind FR-1's friendly refusal: the insert's
/// only unique constraint is the (`party_id`, `item_name`) identity key.
fn is_stash_name_violation(err: &sqlx::Error) -> bool {
    err.as_database_error().is_some_and(|db| {
        db.is_unique_violation() && db.constraint() == Some("party_stash_party_id_item_name_key")
    })
}

/// The create's modifier set: the corpus row resolved at apply time, or the
/// hand-built payload as validated.
async fn resolve_create_modifiers(
    pool: &PgPool,
    create: &EffectCreate,
) -> Result<(Vec<hireling_engine::model::Modifier>, bool, Option<i64>), EffectWriteError> {
    let Some(entry_id) = create.corpus_entry_id else {
        return Ok((create.modifiers.clone(), false, None));
    };
    if !create.modifiers.is_empty() {
        return Err(EffectWriteError::Rejected(
            "corpus-sourced conditions take no inline modifiers \u{2014} the corpus data rules"
                .to_owned(),
        ));
    }
    let row: Option<(Option<String>, Option<JsonValue>)> = sqlx::query_as(
        "SELECT data->'import'->>'tier', modifiers FROM corpus_entries \
         WHERE id = $1 AND kind = 'condition'",
    )
    .bind(entry_id)
    .fetch_optional(pool)
    .await
    .map_err(EffectWriteError::from)?;
    let Some((tier, mappings)) = row else {
        return Err(EffectWriteError::Rejected(format!(
            "corpus_entry_id {entry_id} is not a condition row"
        )));
    };
    let resolved = apply_condition(tier.as_deref(), mappings.as_ref(), create.condition_value)
        .map_err(|apply_error| EffectWriteError::Rejected(apply_error.problem))?;
    for modifier in &resolved.modifiers {
        let value = i64::from(modifier.value);
        if !(-50..=50).contains(&value) {
            return Err(EffectWriteError::Rejected(format!(
                "resolved modifier to `{}` is {value}, outside \u{2212}50..=50",
                modifier.stat
            )));
        }
    }
    Ok((
        resolved.modifiers,
        resolved.tracked_manually,
        Some(entry_id),
    ))
}

/// The typed create payload — parsed after bounds validated the shape.
#[derive(Debug, Clone)]
struct EffectCreate {
    name: String,
    source_character_id: i64,
    targets: Vec<i64>,
    modifiers: Vec<hireling_engine::model::Modifier>,
    duration_note: String,
    corpus_entry_id: Option<i64>,
    condition_value: Option<i64>,
}

fn parse_create(value: &JsonValue) -> Result<EffectCreate, String> {
    let name = value
        .get("name")
        .and_then(JsonValue::as_str)
        .ok_or("create needs a name")?
        .to_owned();
    let source_character_id = value
        .get("source_character_id")
        .and_then(JsonValue::as_i64)
        .ok_or("create needs an integer source_character_id")?;
    let targets = parse_targets(value)?;
    let mut modifiers = Vec::new();
    if let Some(rows) = value.get("modifiers").and_then(JsonValue::as_array) {
        for row in rows {
            let modifier_type = row
                .get("type")
                .and_then(JsonValue::as_str)
                .ok_or("modifier needs a type (circumstance, status, item, untyped)")?;
            let stat = row
                .get("stat")
                .and_then(JsonValue::as_str)
                .ok_or("modifier needs a stat")?;
            let modifier_value = row
                .get("value")
                .and_then(JsonValue::as_i64)
                .ok_or("modifier needs an integer value")?;
            modifiers.push(
                crate::engine_host::parse_modifier_row(
                    "create modifier",
                    modifier_type,
                    stat,
                    i32::try_from(modifier_value)
                        .map_err(|source| format!("modifier value does not fit i32 ({source})"))?,
                )
                .map_err(|err| err.to_string())?,
            );
        }
    }
    let duration_note = value
        .get("duration_note")
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .to_owned();
    let corpus_entry_id = value.get("corpus_entry_id").and_then(JsonValue::as_i64);
    let condition_value = value.get("condition_value").and_then(JsonValue::as_i64);
    Ok(EffectCreate {
        name,
        source_character_id,
        targets,
        modifiers,
        duration_note,
        corpus_entry_id,
        condition_value,
    })
}

fn parse_targets(value: &JsonValue) -> Result<Vec<i64>, String> {
    value
        .get("targets")
        .and_then(JsonValue::as_array)
        .ok_or("create and update need a targets array")?
        .iter()
        .map(|target| {
            target
                .as_i64()
                .ok_or_else(|| "targets must be integers".to_owned())
        })
        .collect()
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
            broadcast: Vec::new(),
            affected: Vec::new(),
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
        broadcast: Vec::new(),
        affected: Vec::new(),
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
        broadcast: Vec::new(),
        affected: Vec::new(),
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
        // the match is honest: the effect's own row (its creator's sub is
        // the authorization fact). Unreachable through `apply_write` —
        // `apply_effect_write` owns effects from step 1b on.
        FieldTarget::Effect { effect_id } => {
            let row: Option<(String, i64)> = sqlx::query_as(
                "SELECT c.owner_sub, e.party_id FROM effects e \
                 JOIN characters c ON c.id = e.source_character_id WHERE e.id = $1",
            )
            .bind(effect_id)
            .fetch_optional(pool)
            .await
            .context("resolve effect target")?;
            Ok(row.map(|(owner_sub, party_id)| TargetRow {
                owner_sub,
                party_id: Some(party_id),
                field_path: format!("effect:{effect_id}"),
            }))
        }
        // Creates are answered by `apply_effect_create`; the party target
        // resolves no row. Unreachable through `apply_write`.
        FieldTarget::EffectNew { party_id } => Ok(Some(TargetRow {
            owner_sub: String::new(),
            party_id: Some(*party_id),
            field_path: format!("effect_new:{party_id}"),
        })),
        // The E12 party targets are routed to `apply_party_write` before
        // resolution (their authz keys on party membership, not a row
        // owner); these arms keep the match honest and are unreachable
        // through `apply_write`.
        FieldTarget::StashNew { .. }
        | FieldTarget::Stash { .. }
        | FieldTarget::Bank { .. }
        | FieldTarget::Party { .. }
        | FieldTarget::Claim { .. } => Ok(Some(TargetRow {
            owner_sub: String::new(),
            party_id: None,
            field_path: field_path(target),
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
        // Effect ops route through `apply_effect_write` (step 1b) before
        // any CAS runs; these arms keep the match honest.
        FieldTarget::Effect { .. } | FieldTarget::EffectNew { .. } => {
            Ok((Outcome::Rejected, None, None))
        }
        // The E12 party targets route through `apply_party_write` (step
        // 1c) — the additive applies carry no version predicate and the CAS
        // ones ride the party engine; this arm keeps the match honest.
        FieldTarget::StashNew { .. }
        | FieldTarget::Stash { .. }
        | FieldTarget::Bank { .. }
        | FieldTarget::Party { .. }
        | FieldTarget::Claim { .. } => Ok((Outcome::Rejected, None, None)),
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
        VitalsField::Money => cas_money(tx, character_id, base_version, value).await?,
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
        // The Raise-a-Shield toggle (gh#82): bounds guaranteed a boolean.
        // NULL is the never-toggled state — the CAS always writes true/false,
        // so a live lower is `false`, never a reset to NULL.
        VitalsField::ShieldRaised => sqlx::query_scalar(
            "UPDATE character_vitals SET shield_raised = $1, \
                 shield_raised_version = nextval('field_version_seq'), updated_at = now() \
                 WHERE character_id = $2 AND shield_raised_version = $3 \
                 RETURNING shield_raised_version",
        )
        .bind(value.as_bool())
        .bind(character_id)
        .bind(base_version)
        .fetch_optional(&mut **tx)
        .await
        .context("CAS shield_raised")?,
    };
    let Some(version) = new_version else {
        let winning = read_vitals_version(tx, character_id, field).await?;
        return Ok((Outcome::Superseded, None, Some(winning)));
    };
    Ok((Outcome::Applied, Some(version), None))
}

/// CAS the money unit: the contract gives the four denominations ONE
/// version unit (E2's schema), so its write is four columns under one
/// `money_version` — its own function, unlike every single-column arm.
async fn cas_money(
    tx: &mut Transaction<'_, Postgres>,
    character_id: i64,
    base_version: i64,
    value: &JsonValue,
) -> anyhow::Result<Option<i64>> {
    let money = MoneyValue::from_json(value);
    let new_version: Option<i64> = sqlx::query_scalar(
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
    .context("CAS money")?;
    Ok(new_version)
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
        VitalsField::ShieldRaised => sqlx::query_scalar(
            "SELECT shield_raised_version FROM character_vitals WHERE character_id = $1",
        )
        .bind(character_id)
        .fetch_one(&mut **tx)
        .await
        .context("re-read shield_raised_version"),
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
/// `vitals:hp`, `slot:Wizard:3:0`, `inv:Chalk`, `effect:42`, and the E12
/// party paths (party-scoped: the stash is shared, the ledger names it).
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
        FieldTarget::EffectNew { party_id } => format!("effect_new:{party_id}"),
        FieldTarget::StashNew { party_id } => format!("stash_new:{party_id}"),
        FieldTarget::Stash {
            party_id,
            item_name,
        } => format!("stash:{party_id}:{item_name}"),
        FieldTarget::Bank { party_id } => format!("bank:{party_id}"),
        FieldTarget::Party { party_id, field } => {
            format!("party:{}:{party_id}", field.as_str())
        }
        FieldTarget::Claim { claim_id } => format!("claim:{claim_id}"),
    }
}

/// The bounds table (plan T3): reject, never silently fix. Pure.
/// The whole-row spell-economy write: exact shape, each field in range.
fn validate_daily(value: &JsonValue) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err(
            "daily must be an object {staff_charge_rank, staff_spent, drain_used}".to_owned(),
        );
    };
    if object.len() != 3 {
        return Err("daily needs exactly staff_charge_rank, staff_spent, drain_used".to_owned());
    }
    match object.get("staff_charge_rank").and_then(JsonValue::as_i64) {
        Some(rank) if (0..=10).contains(&rank) => {}
        _ => {
            return Err("daily.staff_charge_rank must be an integer within 0\u{2014}10".to_owned());
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

/// The write path's own bounds gate; the wire-contract suite pins the
/// fixture's `effect_ops` through it (gh#90 F3), so it is crate-visible.
pub(crate) fn validate_bounds(target: &FieldTarget, value: &JsonValue) -> Result<(), String> {
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
            VitalsField::Daily => validate_daily(value),
            VitalsField::ShieldRaised => match value {
                JsonValue::Bool(_) => Ok(()),
                JsonValue::Null
                | JsonValue::Number(_)
                | JsonValue::String(_)
                | JsonValue::Array(_)
                | JsonValue::Object(_) => Err("shield_raised must be a boolean".to_owned()),
            },
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
        // Effect writes decode since E8; this is the write path's own gate
        // — per-op, deny-by-default, with the `rejected` reasons (plan T7).
        FieldTarget::EffectNew { .. } => validate_effect_bounds(true, value),
        FieldTarget::Effect { .. } => validate_effect_bounds(false, value),
        // E12's party targets (design D2): the mint, the row ops, the bank
        // delta, the designation. The claim target is read-only — append-only
        // rows have no write frame.
        FieldTarget::StashNew { .. } => validate_stash_new(value),
        FieldTarget::Stash { .. } => validate_stash(value),
        FieldTarget::Bank { .. } => validate_bank(value),
        FieldTarget::Party { .. } => validate_party(value),
        FieldTarget::Claim { .. } => {
            Err("claim rows are append-only and cannot be written".to_owned())
        }
    }
}

/// The effect target's own bounds (E8, plan T7): per-op, deny-by-default.
/// `create_target` is the `effect_new` frame (`create` only); the effect
/// frame carries `update`/`end`. The database-dependent checks (roster
/// membership, ownership, corpus rows) live in the effect engine, where
/// the rows are.
fn validate_effect_bounds(create_target: bool, value: &JsonValue) -> Result<(), String> {
    let op = value.get("op").and_then(JsonValue::as_str).unwrap_or("");
    if create_target {
        if op != "create" {
            return Err(format!(
                "unknown effect op {op:?} on effect_new (create goes on effect_new; update and end on effect)"
            ));
        }
        validate_effect_value(value, "create")
    } else {
        match op {
            "update" => validate_effect_value(value, "update"),
            "end" => {
                if value.as_object().is_some_and(|object| object.len() != 1) {
                    return Err("end carries no fields".to_owned());
                }
                Ok(())
            }
            other => Err(format!(
                "unknown effect op {other:?} (create goes on effect_new; update and end on effect)"
            )),
        }
    }
}

/// The `stash_new` mint payload: `{item_name, quantity > 0, bulk?, notes?}`
/// (D2). The quantity floor is the shipped CHECK's (> 0) — a zero-quantity
/// row is deleted, not kept.
fn validate_stash_new(value: &JsonValue) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("stash_new value must be an object".to_owned());
    };
    for key in object.keys() {
        if !matches!(key.as_str(), "item_name" | "quantity" | "bulk" | "notes") {
            return Err(format!("stash_new value: unknown key {key:?}"));
        }
    }
    match object.get("item_name").and_then(JsonValue::as_str) {
        Some(name) if !name.is_empty() => {}
        _ => return Err("stash_new needs a non-empty item_name".to_owned()),
    }
    match object
        .get("quantity")
        .and_then(JsonValue::as_i64)
        .and_then(|quantity| i32::try_from(quantity).ok())
    {
        Some(quantity) if quantity >= 1 => {}
        _ => {
            return Err(
                "stash_new.quantity must be an integer \u{2265} 1 (a zero-quantity row is deleted, not kept)"
                    .to_owned(),
            )
        }
    }
    validate_stash_bulk(object.get("bulk"))?;
    if let Some(notes) = object.get("notes")
        && !notes.is_string()
    {
        return Err("stash_new.notes must be a string".to_owned());
    }
    Ok(())
}

/// The stash row's op-discriminated value (D2): `{op:"qty", qty_delta}` or
/// `{op:"meta", bulk?, notes?}` at this lane; Task 3's transfer/sell join
/// the accepted set when they land.
fn validate_stash(value: &JsonValue) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("stash value must be an object carrying op".to_owned());
    };
    match object.get("op").and_then(JsonValue::as_str) {
        Some("qty") => {
            if object.len() != 2 || !object.contains_key("qty_delta") {
                return Err("stash qty value must be exactly {op, qty_delta}".to_owned());
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
        Some("meta") => {
            for key in object.keys() {
                if !matches!(key.as_str(), "op" | "bulk" | "notes") {
                    return Err(format!("stash meta value: unknown key {key:?}"));
                }
            }
            if !object.contains_key("bulk") && !object.contains_key("notes") {
                return Err("stash meta needs bulk or notes".to_owned());
            }
            validate_stash_bulk(object.get("bulk"))?;
            if let Some(notes) = object.get("notes")
                && !notes.is_string()
            {
                return Err("stash meta notes must be a string".to_owned());
            }
            Ok(())
        }
        Some(other) => Err(format!(
            "unknown stash op {other:?} (qty and meta are the wired ops)"
        )),
        None => Err("stash value needs an op".to_owned()),
    }
}

/// `party_stash.bulk` is numeric(6,1): 0..=99999.9, absent/null = negligible
/// (the shipped convention; "L" = 0.1).
fn validate_stash_bulk(value: Option<&JsonValue>) -> Result<(), String> {
    match value {
        None | Some(JsonValue::Null) => Ok(()),
        Some(bulk) => match bulk.as_f64() {
            Some(number) if (0.0..=99_999.9).contains(&number) => Ok(()),
            Some(_) => Err(
                "bulk must be a number within 0\u{2014}99999.9 (numeric(6,1); \"L\" = 0.1)"
                    .to_owned(),
            ),
            None => Err("bulk must be a number".to_owned()),
        },
    }
}

/// The bank's signed per-denomination delta: exactly `{gp_delta, sp_delta,
/// cp_delta}`, each an i32-range integer (the shipped int columns).
fn validate_bank(value: &JsonValue) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("bank value must be an object {gp_delta, sp_delta, cp_delta}".to_owned());
    };
    if object.len() != 3 {
        return Err("bank needs exactly gp_delta, sp_delta, cp_delta".to_owned());
    }
    for key in ["gp_delta", "sp_delta", "cp_delta"] {
        match object
            .get(key)
            .and_then(JsonValue::as_i64)
            .and_then(|delta| i32::try_from(delta).ok())
        {
            Some(_) => {}
            None => return Err(format!("bank.{key} must be a signed integer")),
        }
    }
    Ok(())
}

/// The quartermaster write: exactly `{character_id}` — an integer (roster
/// existence is the write arm's check, where the rows are) or null (clear).
fn validate_party(value: &JsonValue) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("party value must be exactly {character_id}".to_owned());
    };
    if object.len() != 1 {
        return Err("party value must be exactly {character_id}".to_owned());
    }
    let Some(character_id) = object.get("character_id") else {
        return Err("party value must be exactly {character_id}".to_owned());
    };
    if character_id.is_null() || character_id.as_i64().is_some() {
        Ok(())
    } else {
        Err("character_id must be an integer or null".to_owned())
    }
}

/// The pure half of effect-op validation: shape, sizes, the closed
/// vocabulary, the ranges. `kind` is `create` or `update` — the keys each
/// op may carry.
fn validate_effect_value(value: &JsonValue, kind: &str) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("effect value must be an object".to_owned());
    };
    ensure_known_keys(object, kind)?;
    if kind == "create" {
        validate_create_keys(object)?;
    }
    validate_targets(object)?;
    validate_modifiers(object)?;
    Ok(())
}

/// The per-op key allowlist: anything outside it is a client bug, named
/// loudly (deny-by-default, never silently ignored).
fn ensure_known_keys(
    object: &serde_json::Map<String, JsonValue>,
    kind: &str,
) -> Result<(), String> {
    let allowed: &[&str] = if kind == "create" {
        &[
            "op",
            "name",
            "source_character_id",
            "targets",
            "modifiers",
            "duration_note",
            "corpus_entry_id",
            "condition_value",
        ]
    } else {
        &["op", "targets"]
    };
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("{kind} value: unknown key {key:?}"));
        }
    }
    Ok(())
}

/// The create-only keys: name, source, the corpus pair, the duration note.
fn validate_create_keys(object: &serde_json::Map<String, JsonValue>) -> Result<(), String> {
    let name = object
        .get("name")
        .and_then(JsonValue::as_str)
        .ok_or("create needs a name")?;
    if name.is_empty() || name.chars().count() > 120 {
        return Err("name must be 1..=120 characters".to_owned());
    }
    let source = object
        .get("source_character_id")
        .and_then(JsonValue::as_i64)
        .ok_or("create needs an integer source_character_id")?;
    if source <= 0 {
        return Err("source_character_id must be a character id".to_owned());
    }
    if let Some(duration) = object.get("duration_note")
        && !duration.is_string()
    {
        return Err("duration_note must be a string".to_owned());
    }
    let corpus_entry_id = object.get("corpus_entry_id");
    let condition_value = object.get("condition_value");
    if let Some(entry) = corpus_entry_id
        && !entry.is_null()
        && !entry.is_i64()
    {
        return Err("corpus_entry_id must be an integer or null".to_owned());
    }
    if let Some(value) = condition_value
        && !value.is_null()
        && !value.is_i64()
    {
        return Err("condition_value must be an integer or null".to_owned());
    }
    let corpus_sourced = corpus_entry_id.is_some_and(JsonValue::is_i64);
    if corpus_sourced
        && let Some(modifiers) = object.get("modifiers")
        && modifiers.as_array().is_some_and(|rows| !rows.is_empty())
    {
        // Corpus-sourced: a condition value may ride; inline modifiers may
        // not (the corpus data rules, D7).
        return Err(
            "corpus-sourced conditions take no inline modifiers \u{2014} the corpus data rules"
                .to_owned(),
        );
    }
    if let Some(supplied) = object.get("condition_value")
        && !supplied.is_null()
    {
        match supplied.as_i64() {
            Some(value) if (0..=50).contains(&value) => {}
            Some(_) => return Err("condition_value must be within 0\u{2014}50".to_owned()),
            None => return Err("condition_value must be an integer or null".to_owned()),
        }
    }
    Ok(())
}

/// Targets: an array of at least one distinct positive character id,
/// present in both create and update. The minimum is the ghost-effect
/// guard: an accepted empty set would leave an active effect that
/// recomputes nothing, shows in nobody's chips, and only `end` can clear
/// (gh#85).
fn validate_targets(object: &serde_json::Map<String, JsonValue>) -> Result<(), String> {
    let rows = object
        .get("targets")
        .and_then(JsonValue::as_array)
        .ok_or("targets must be an array of character ids")?;
    if rows.is_empty() {
        return Err("an effect needs at least one target".to_owned());
    }
    let mut seen = std::collections::HashSet::with_capacity(rows.len());
    for target in rows {
        let id = target
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or("targets must be positive character ids")?;
        if !seen.insert(id) {
            return Err(format!("target {id} appears twice"));
        }
    }
    Ok(())
}

/// Modifiers (create only): at most 16, each exactly `{type, stat, value}`,
/// the type in the closed four, the stat in the engine's vocabulary, the
/// value an integer within −50..=50. One validator for the whole row
/// (`engine_host::parse_modifier_row`), so the bounds and the write path
/// cannot drift (D2).
fn validate_modifiers(object: &serde_json::Map<String, JsonValue>) -> Result<(), String> {
    let Some(rows) = object.get("modifiers").and_then(JsonValue::as_array) else {
        return Ok(());
    };
    if rows.len() > 16 {
        return Err("an effect carries at most 16 modifiers".to_owned());
    }
    for row in rows {
        let Some(entry) = row.as_object() else {
            return Err("each modifier is an object {type, stat, value}".to_owned());
        };
        if entry.len() != 3 {
            return Err("each modifier is exactly {type, stat, value}".to_owned());
        }
        let modifier_type = entry
            .get("type")
            .and_then(JsonValue::as_str)
            .ok_or("modifier needs a type")?;
        let stat = entry
            .get("stat")
            .and_then(JsonValue::as_str)
            .ok_or("modifier needs a stat")?;
        let value: i32 = entry
            .get("value")
            .and_then(JsonValue::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .filter(|value| (-50..=50).contains(value))
            .ok_or("modifier value must be an integer within \u{2212}50..=50")?;
        crate::engine_host::parse_modifier_row("modifier", modifier_type, stat, value)
            .map_err(|err| err.to_string())?;
    }
    Ok(())
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
    fn vitals_bounds_reject_and_accept_exactly() {
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

        let shield = |value| validate_bounds(&vitals(VitalsField::ShieldRaised), &value);
        assert!(shield(json!(true)).is_ok());
        assert!(shield(json!(false)).is_ok(), "lowered is a real live state");
        assert!(shield(json!("yes")).is_err(), "must be a boolean");
        assert!(shield(json!(1)).is_err(), "must be a boolean");

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
    }

    #[test]
    fn slot_and_inv_bounds_reject_and_accept_exactly() {
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
    }

    #[test]
    fn party_target_bounds_reject_and_accept_exactly() {
        // E12's party targets (design D2).
        let mint = |value| validate_bounds(&FieldTarget::StashNew { party_id: 1 }, &value);
        assert!(mint(json!({"item_name": "Rope", "quantity": 1})).is_ok());
        assert!(
            mint(json!({"item_name": "Rope", "quantity": 1, "bulk": 0.1, "notes": "x"})).is_ok()
        );
        assert!(
            mint(json!({"item_name": "Rope", "quantity": 0})).is_err(),
            "quantity > 0 is the shipped CHECK"
        );
        assert!(mint(json!({"item_name": "Rope", "quantity": -1})).is_err());
        assert!(mint(json!({"quantity": 1})).is_err(), "item_name required");
        assert!(
            mint(json!({"item_name": "", "quantity": 1})).is_err(),
            "empty name refused"
        );
        assert!(
            mint(json!({"item_name": "Rope", "quantity": 1, "bulk": -0.1})).is_err(),
            "negative bulk refused"
        );
        assert!(
            mint(json!({"item_name": "Rope", "quantity": 1, "bulk": 100_000.0})).is_err(),
            "numeric(6,1) ceiling"
        );
        assert!(mint(json!({"item_name": "Rope", "quantity": 1, "notes": 3})).is_err());
        assert!(
            mint(json!({"item_name": "Rope", "quantity": 1, "wat": 1})).is_err(),
            "unknown keys denied"
        );

        let stash = |value| {
            validate_bounds(
                &FieldTarget::Stash {
                    party_id: 1,
                    item_name: "Rope".to_owned(),
                },
                &value,
            )
        };
        assert!(stash(json!({"op": "qty", "qty_delta": -2})).is_ok());
        assert!(
            stash(json!({"op": "qty"})).is_err(),
            "exactly {{op, qty_delta}}"
        );
        assert!(stash(json!({"op": "qty", "qty_delta": 1, "extra": 1})).is_err());
        assert!(stash(json!({"op": "qty", "qty_delta": "two"})).is_err());
        assert!(stash(json!({"op": "meta", "notes": "x"})).is_ok());
        assert!(stash(json!({"op": "meta", "bulk": 0.5})).is_ok());
        assert!(
            stash(json!({"op": "meta", "bulk": null})).is_ok(),
            "null clears to negligible"
        );
        assert!(
            stash(json!({"op": "meta"})).is_err(),
            "needs at least one field"
        );
        assert!(stash(json!({"op": "meta", "bulk": -1})).is_err());
        assert!(
            stash(json!({"op": "sell", "qty": 1})).is_err(),
            "sell is lane L3, not wired yet"
        );
        assert!(stash(json!({"extra": 1})).is_err());

        let bank = |value| validate_bounds(&FieldTarget::Bank { party_id: 1 }, &value);
        assert!(
            bank(json!({"gp_delta": 1, "sp_delta": -2, "cp_delta": 0})).is_ok(),
            "signed deltas fine"
        );
        assert!(
            bank(json!({"gp_delta": 1, "sp_delta": 2})).is_err(),
            "all three"
        );
        assert!(bank(json!({"gp_delta": "x", "sp_delta": 2, "cp_delta": 0})).is_err());

        let party = |value| {
            validate_bounds(
                &FieldTarget::Party {
                    party_id: 1,
                    field: PartyField::Quartermaster,
                },
                &value,
            )
        };
        assert!(party(json!({"character_id": 3})).is_ok());
        assert!(
            party(json!({"character_id": null})).is_ok(),
            "clearing is a legal write"
        );
        assert!(party(json!({"character_id": "three"})).is_err());
        assert!(party(json!({"character_id": 3, "extra": 1})).is_err());
        assert!(party(json!({})).is_err());

        let claim = validate_bounds(&FieldTarget::Claim { claim_id: 1 }, &json!(null));
        assert!(
            claim.is_err(),
            "claim rows are append-only — no write frame"
        );
    }

    #[test]
    fn the_effect_bounds_reject_and_accept_exactly() {
        // E8's own bounds: per-op, deny-by-default.
        let effect = |value| validate_bounds(&FieldTarget::Effect { effect_id: 7 }, &value);
        assert!(
            effect(json!({"op": "end"})).is_ok(),
            "end is the whole payload"
        );
        assert!(effect(json!({"op": "end", "targets": [3]})).is_err());
        assert!(
            effect(json!({"op": "retire"})).is_err(),
            "unknown ops are denied at bounds"
        );
        assert!(
            effect(json!({"op": "update", "targets": [3, 3]})).is_err(),
            "duplicate targets are a client bug, named loudly"
        );
        let empty_update = effect(json!({"op": "update", "targets": []}));
        assert!(
            matches!(&empty_update, Err(reason) if reason.contains("at least one target")),
            "an empty target set is refused with its reason: {empty_update:?}"
        );
        assert!(
            effect(json!({"op": "update", "targets": [3], "name": "x"})).is_err(),
            "update carries no create keys"
        );

        let create = |value| validate_bounds(&FieldTarget::EffectNew { party_id: 1 }, &value);
        let bless = json!({"op": "create", "name": "Bless", "source_character_id": 3,
                           "targets": [7, 9],
                           "modifiers": [{"type": "status", "stat": "attack", "value": 1}],
                           "duration_note": "10 rounds", "corpus_entry_id": null,
                           "condition_value": null});
        assert!(
            create(bless.clone()).is_ok(),
            "the contract's own example applies"
        );
        assert!(create(json!({"op": "create"})).is_err());
        let long_name = "x".repeat(121);
        assert!(
            create(
                json!({"op": "create", "name": long_name, "source_character_id": 3,
                          "targets": [3]})
            )
            .is_err(),
            "name ≤ 120"
        );
        let many: Vec<JsonValue> = (0..17)
            .map(|_| json!({"type": "status", "stat": "attack", "value": 1}))
            .collect();
        assert!(
            create(
                json!({"op": "create", "name": "x", "source_character_id": 3,
                          "targets": [3], "modifiers": many})
            )
            .is_err(),
            "≤ 16 modifiers"
        );
        assert!(
            create(
                json!({"op": "create", "name": "x", "source_character_id": 3,
                          "targets": [3],
                          "modifiers": [{"type": "status", "stat": "initiative", "value": 1}]})
            )
            .is_err(),
            "stats validate against the closed vocabulary"
        );
        assert!(
            create(
                json!({"op": "create", "name": "x", "source_character_id": 3,
                          "targets": [3],
                          "modifiers": [{"type": "status", "stat": "attack", "value": 51}]})
            )
            .is_err(),
            "values ∈ −50..=50"
        );
        assert!(
            create(
                json!({"op": "create", "name": "x", "source_character_id": 3,
                          "targets": [3], "corpus_entry_id": 5,
                          "modifiers": [{"type": "status", "stat": "attack", "value": 1}]})
            )
            .is_err(),
            "corpus-sourced creates take no inline modifiers"
        );
        let empty_create = create(
            json!({"op": "create", "name": "Bless", "source_character_id": 3,
                          "targets": [],
                          "modifiers": [{"type": "status", "stat": "attack", "value": 1}],
                          "duration_note": "10 rounds", "corpus_entry_id": null,
                          "condition_value": null}),
        );
        assert!(
            matches!(&empty_create, Err(reason) if reason.contains("at least one target")),
            "a zero-target create is refused with its reason: {empty_create:?}"
        );
    }

    fn vitals(field: VitalsField) -> FieldTarget {
        FieldTarget::Vitals {
            character_id: 1,
            field,
        }
    }
}
