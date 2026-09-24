-- E2 migration 3/5: effects.
--
-- A named, creator-owned buff/condition/penalty with target roster
-- characters, typed modifiers, a duration note, and an active/ended state
-- (FR-10). Versioned as a whole (FR-14): any target/modifier/active change
-- bumps effects.version in the same transaction.
--
-- Field-level truth: specs/002-database-schema/data-model.md §4.

CREATE TABLE effects (
    id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- Denormalized from the source character so party queries never join
    -- through (FR-4).
    party_id bigint NOT NULL REFERENCES parties (id) ON DELETE CASCADE,
    -- The creator/owner. RESTRICT, not CASCADE: removing a creator while
    -- their effects exist is a human decision (end or reassign first) — an
    -- effect is never silently destroyed by another entity's removal.
    source_character_id bigint NOT NULL REFERENCES characters (id) ON DELETE RESTRICT,
    name                text NOT NULL,
    -- Free text ("10 rounds", "while in aura") — displayed, never enforced
    -- (PRD: no countdown automation).
    duration_note       text NOT NULL DEFAULT '',
    -- Ended = false, kept queryable — state, not deletion (FR-15).
    active              boolean NOT NULL DEFAULT true,
    version             bigint NOT NULL DEFAULT nextval('field_version_seq'),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX effects_party_id_idx ON effects (party_id);
CREATE INDEX effects_source_character_id_idx ON effects (source_character_id);

-- Roster characters only — companions/minions are not targetable (PRD FG3).
-- The link may die with a removed roster character; the effect row may not.
CREATE TABLE effect_targets (
    effect_id    bigint NOT NULL REFERENCES effects (id) ON DELETE CASCADE,
    character_id bigint NOT NULL REFERENCES characters (id) ON DELETE CASCADE,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (effect_id, character_id)
);

CREATE INDEX effect_targets_character_id_idx ON effect_targets (character_id);

-- Ordered modifier list (FR-10). Penalties are negative values, not a type.
CREATE TABLE effect_modifiers (
    effect_id bigint NOT NULL REFERENCES effects (id) ON DELETE CASCADE,
    ord       int NOT NULL CHECK (ord >= 0),
    type      text NOT NULL CHECK (type IN ('circumstance', 'status', 'item', 'untyped')),
    -- The closed engine vocabulary — single stats, blanket targets
    -- (all_checks, all_dcs, all_checks_and_dcs), and `skill:<name>`. The
    -- vocabulary is closed but skill names are open-ended, so the schema
    -- stores text and E8 owns enforcement — a partial CHECK would lie.
    stat      text NOT NULL CHECK (length(stat) > 0),
    value     int NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (effect_id, ord)
);
