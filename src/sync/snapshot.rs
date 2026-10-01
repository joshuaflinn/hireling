//! The party live-state snapshot — E7's catch-up read (plan Task 4).
//!
//! One SELECT per table, all party-scoped: every versioned field of every
//! character in the party plus the effect rows (read-only in E7), emitted
//! as [`SnapshotField`]s whose versions are what the client merges
//! strictly-newer against (contract §3/§4). Reconnect or cold boot gets the
//! whole picture and keeps only what is newer; there is no delta log.

use anyhow::Context as _;
use serde_json::{Value as JsonValue, json};
use sqlx::{PgPool, Row as _};

use crate::sync::protocol::{FieldTarget, SnapshotField, VitalsField};

/// Every versioned live-state field of `party_id`: per character the four
/// vitals fields, every spell-slot row, every inventory row; then one row
/// per effect (value shape per contract §3:
/// `{name, source_character_id, targets[], modifiers[], duration_note,
/// active}` — the effect id rides the target, not the value). Deterministic
/// order: characters by id, fields in table order, effects by id.
///
/// # Errors
///
/// Database failures propagate (`anyhow`).
pub async fn party_snapshot(pool: &PgPool, party_id: i64) -> anyhow::Result<Vec<SnapshotField>> {
    let mut fields = vitals_fields(pool, party_id).await?;
    fields.extend(slot_fields(pool, party_id).await?);
    fields.extend(inventory_fields(pool, party_id).await?);
    fields.extend(effect_fields(pool, party_id).await?);
    Ok(fields)
}

/// The four vitals fields per character: `hp`, `temp_hp`, `money` (one versioned
/// unit of four denominations — E2's schema), `level_adjust`.
async fn vitals_fields(pool: &PgPool, party_id: i64) -> anyhow::Result<Vec<SnapshotField>> {
    let rows = sqlx::query(
        "SELECT c.id, v.hp, v.hp_version, v.temp_hp, v.temp_hp_version, \
                v.money_pp, v.money_gp, v.money_sp, v.money_cp, v.money_version, \
                v.level_adjust, v.level_adjust_version \
         FROM characters c JOIN character_vitals v ON v.character_id = c.id \
         WHERE c.party_id = $1 ORDER BY c.id",
    )
    .bind(party_id)
    .fetch_all(pool)
    .await
    .context("snapshot vitals")?;
    let mut fields = Vec::new();
    for row in rows {
        let character_id: i64 = row.get("id");
        let hp: i32 = row.get("hp");
        let temp_hp: i32 = row.get("temp_hp");
        let money = json!({
            "pp": row.get::<i32, _>("money_pp"),
            "gp": row.get::<i32, _>("money_gp"),
            "sp": row.get::<i32, _>("money_sp"),
            "cp": row.get::<i32, _>("money_cp"),
        });
        let level_adjust: i32 = row.get("level_adjust");
        for (field, value, version_column) in [
            (VitalsField::Hp, json!(hp), "hp_version"),
            (VitalsField::TempHp, json!(temp_hp), "temp_hp_version"),
            (VitalsField::Money, money, "money_version"),
            (
                VitalsField::LevelAdjust,
                json!(level_adjust),
                "level_adjust_version",
            ),
        ] {
            fields.push(SnapshotField {
                field: FieldTarget::Vitals {
                    character_id,
                    field,
                },
                value,
                version: row.get(version_column),
            });
        }
    }
    Ok(fields)
}

/// Every spell-slot row: the value is the whole-slot state — unmentioned
/// fields reset on write, so the snapshot carries both keys always.
async fn slot_fields(pool: &PgPool, party_id: i64) -> anyhow::Result<Vec<SnapshotField>> {
    let rows = sqlx::query(
        "SELECT s.character_id, s.caster_key, s.rank, s.slot_index, \
                s.used, s.prepared_spell, s.version \
         FROM character_spell_slots s JOIN characters c ON c.id = s.character_id \
         WHERE c.party_id = $1 \
         ORDER BY s.character_id, s.caster_key, s.rank, s.slot_index",
    )
    .bind(party_id)
    .fetch_all(pool)
    .await
    .context("snapshot slots")?;
    let mut fields = Vec::new();
    for row in rows {
        let character_id: i64 = row.get("character_id");
        let caster_key: String = row.get("caster_key");
        let rank: i32 = row.get("rank");
        let slot_index: i32 = row.get("slot_index");
        let used: bool = row.get("used");
        let prepared: Option<String> = row.get("prepared_spell");
        fields.push(SnapshotField {
            field: FieldTarget::Slot {
                character_id,
                caster_key,
                rank,
                slot_index,
            },
            value: json!({"used": used, "prepared": prepared}),
            version: row.get("version"),
        });
    }
    Ok(fields)
}

/// Every inventory row: the value is the stored signed delta —
/// absolute-set semantics like every other target.
async fn inventory_fields(pool: &PgPool, party_id: i64) -> anyhow::Result<Vec<SnapshotField>> {
    let rows = sqlx::query(
        "SELECT i.character_id, i.item_name, i.qty_delta, i.version \
         FROM character_inventory_live i JOIN characters c ON c.id = i.character_id \
         WHERE c.party_id = $1 ORDER BY i.character_id, i.item_name",
    )
    .bind(party_id)
    .fetch_all(pool)
    .await
    .context("snapshot inventory")?;
    let mut fields = Vec::new();
    for row in rows {
        let character_id: i64 = row.get("character_id");
        let item_name: String = row.get("item_name");
        let qty_delta: i32 = row.get("qty_delta");
        fields.push(SnapshotField {
            field: FieldTarget::Inv {
                character_id,
                item_name,
            },
            value: json!({"qty_delta": qty_delta}),
            version: row.get("version"),
        });
    }
    Ok(fields)
}

/// The effect rows: whole-row version; the roster and the ord-ordered
/// modifier list ride in the value. Read-only in E7 — they exist here so
/// clients can display them, and E8's writes will version this same row.
async fn effect_fields(pool: &PgPool, party_id: i64) -> anyhow::Result<Vec<SnapshotField>> {
    let rows = sqlx::query(
        "SELECT id, source_character_id, name, duration_note, active, version, tracked_manually \
         FROM effects WHERE party_id = $1 ORDER BY id",
    )
    .bind(party_id)
    .fetch_all(pool)
    .await
    .context("snapshot effects")?;
    let mut fields = Vec::new();
    for row in rows {
        let effect_id: i64 = row.get("id");
        let targets: Vec<i64> = sqlx::query_scalar(
            "SELECT character_id FROM effect_targets WHERE effect_id = $1 ORDER BY character_id",
        )
        .bind(effect_id)
        .fetch_all(pool)
        .await
        .context("snapshot effect targets")?;
        let modifiers: Vec<(String, String, i32)> = sqlx::query_as(
            "SELECT type, stat, value FROM effect_modifiers WHERE effect_id = $1 ORDER BY ord",
        )
        .bind(effect_id)
        .fetch_all(pool)
        .await
        .context("snapshot effect modifiers")?;
        let modifiers_json: Vec<JsonValue> = modifiers
            .into_iter()
            .map(|(modifier_type, stat, value)| {
                json!({"type": modifier_type, "stat": stat, "value": value})
            })
            .collect();
        let source_character_id: i64 = row.get("source_character_id");
        let name: String = row.get("name");
        let duration_note: String = row.get("duration_note");
        let active: bool = row.get("active");
        let tracked_manually: bool = row.get("tracked_manually");
        fields.push(SnapshotField {
            field: FieldTarget::Effect { effect_id },
            value: json!({
                "name": name,
                "source_character_id": source_character_id,
                "targets": targets,
                "modifiers": modifiers_json,
                "duration_note": duration_note,
                "active": active,
                "tracked_manually": tracked_manually
            }),
            version: row.get("version"),
        });
    }
    Ok(fields)
}

/// The `snapshot_bytes` metric (contract §7): the serialized byte length of
/// the snapshot rows.
///
/// # Errors
///
/// Serialization failure — unreachable for [`SnapshotField`]'s closed value
/// shapes, but a metric silently defaulted on an error would lie, so the
/// error propagates.
pub fn snapshot_bytes(fields: &[SnapshotField]) -> anyhow::Result<u64> {
    let bytes = serde_json::to_vec(fields).context("serialize snapshot rows")?;
    Ok(bytes.len() as u64)
}
