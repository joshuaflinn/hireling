-- E2 migration 2/5: live state.
--
-- The mutable, anchored, versioned state that survives re-import (FR-8,
-- FR-9, FR-14). No FK from any of these tables into base-sheet contents —
-- that absence is the mechanism (SC-5): wholesale base replacement cannot
-- cascade-delete or silently orphan live state. Orphaned anchors (renamed
-- item, vanished slot) are kept and surfaced by E5, never dropped.
--
-- Field-level truth: specs/002-database-schema/data-model.md §3.

-- 1:1 with characters; PK is the FK. HP/temp-HP anchor: the row itself
-- (character identity). Max HP lives in base_sheet; clamping is app logic.
CREATE TABLE character_vitals (
    character_id         bigint PRIMARY KEY REFERENCES characters (id) ON DELETE CASCADE,
    hp                   int NOT NULL DEFAULT 0 CHECK (hp >= 0),
    hp_version           bigint NOT NULL DEFAULT nextval('field_version_seq'),
    temp_hp              int NOT NULL DEFAULT 0 CHECK (temp_hp >= 0),
    temp_hp_version      bigint NOT NULL DEFAULT nextval('field_version_seq'),
    money_gp             int NOT NULL DEFAULT 0 CHECK (money_gp >= 0),
    money_sp             int NOT NULL DEFAULT 0 CHECK (money_sp >= 0),
    money_cp             int NOT NULL DEFAULT 0 CHECK (money_cp >= 0),
    money_pp             int NOT NULL DEFAULT 0 CHECK (money_pp >= 0),
    -- Money is live state (spending syncs like HP) but not in FR-14's
    -- per-field granularity list — the four denominations version as one unit.
    money_version        bigint NOT NULL DEFAULT nextval('field_version_seq'),
    -- Delta over the base-sheet level, not an absolute: a re-import at a new
    -- Pathbuilder level preserves the mid-session adjustment. Clamp to 1–20
    -- effective is app logic (PRD FG1).
    level_adjust         int NOT NULL DEFAULT 0 CHECK (level_adjust BETWEEN -19 AND 19),
    level_adjust_version bigint NOT NULL DEFAULT nextval('field_version_seq'),
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now()
);

-- One row per individual slot (FR-14 granularity), anchored by caster block
-- name + rank + index (FR-9, extended by caster_key because the reference
-- export carries two caster blocks with overlapping ranks). The slot layout
-- itself (how many slots per rank) lives in base_sheet; these rows hold only
-- live usage and preparation.
CREATE TABLE character_spell_slots (
    character_id   bigint NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    caster_key     text NOT NULL,
    rank           int NOT NULL CHECK (rank BETWEEN 0 AND 10), -- 0 = cantrip
    slot_index     int NOT NULL CHECK (slot_index >= 0),
    used           boolean NOT NULL DEFAULT false,
    prepared_spell text, -- NULL = unprepared
    version        bigint NOT NULL DEFAULT nextval('field_version_seq'),
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (character_id, caster_key, rank, slot_index)
);

-- Per-item live quantity as a signed delta against the base-sheet quantity.
-- Items have no ids in the export — name is the anchor (FR-9). Container
-- membership and extradimensional flags stay in base_sheet: layout is base
-- data; only quantity is settled-granularity live state.
CREATE TABLE character_inventory_live (
    character_id bigint NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    item_name    text NOT NULL,
    qty_delta    int NOT NULL DEFAULT 0,
    version      bigint NOT NULL DEFAULT nextval('field_version_seq'),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (character_id, item_name)
);
