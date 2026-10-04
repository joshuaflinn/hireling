-- E8 migration 1/1: corpus-linked effects (design D7, FR-8).
--
-- One effects row per applied condition; corpus-resolved modifiers are
-- FROZEN at apply time — a seed-tier flip mid-session never rewrites
-- history (the effect's math was resolved when it was applied and stays).
--
-- Field-level truth: specs/008-buff-effect-engine/design.md §Storage.

-- Display-only corpus condition: badge chip, zero math (US-3). Frozen at
-- apply; a plain false for hand-built effects (the default).
ALTER TABLE effects
    ADD COLUMN tracked_manually boolean NOT NULL DEFAULT false;

-- Provenance link to the corpus row the condition came from (E9 tooltips
-- key off it later). NULL for hand-built effects. The link may die with a
-- removed corpus row; the effect row may not — so RESTRICT, not CASCADE:
-- an effect is never silently severed from its origin by another entity's
-- removal.
ALTER TABLE effects
    ADD COLUMN corpus_entry_id bigint REFERENCES corpus_entries (id)
        ON DELETE RESTRICT;
