-- Dave parity (specs/015, Josh 2026-10-15: "I want all of Dave's panels"):
-- the prototype's per-character UI state that has no home in the E2 schema —
-- Notes pages (rich text, named pages, #tags), current minions (+ HP), and
-- the drag/resize board layout. One whole-row JSON field with its own CAS
-- version, exactly the `daily` pattern (E6 Q1): small, per-character,
-- last-writer-wins on the whole object, validated server-side for shape and
-- size (sync/write.rs validate_sheet_ui).
ALTER TABLE character_vitals
    ADD COLUMN sheet_ui         jsonb  NOT NULL DEFAULT '{"notes": [], "minions": [], "layout": null}'::jsonb,
    ADD COLUMN sheet_ui_version bigint NOT NULL DEFAULT nextval('field_version_seq');
