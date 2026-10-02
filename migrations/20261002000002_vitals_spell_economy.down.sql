-- Down: drop the E6 spell-economy fields in reverse order. The tracked
-- values go with them — live session state, not anchored base data
-- (live_state.sql header).
ALTER TABLE character_vitals
    DROP COLUMN IF EXISTS daily_version,
    DROP COLUMN IF EXISTS daily,
    DROP COLUMN IF EXISTS hero_points_version,
    DROP COLUMN IF EXISTS hero_points,
    DROP COLUMN IF EXISTS focus_version,
    DROP COLUMN IF EXISTS focus_current;
