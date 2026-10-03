-- E6 review fix (MOR-48 finding 11): `daily`'s invariant is structural —
-- one JSON object carrying staff_charge_rank (number), staff_spent
-- (number) and drain_used (boolean). `focus_current` and `hero_points`
-- carry bounds CHECKs at the database; the one whole-row field gets its
-- shape CHECK at the same layer, so a hand-edited or buggy writer cannot
-- persist a malformed row. Each key is required: a missing key is SQL
-- NULL, jsonb_typeof(NULL) is NULL, and a NULL CHECK passes — so the
-- NOT NULL guards are load-bearing, not decoration.
ALTER TABLE character_vitals
    ADD CONSTRAINT character_vitals_daily_shape CHECK (
        jsonb_typeof(daily) = 'object'
        AND (daily -> 'staff_charge_rank') IS NOT NULL
        AND jsonb_typeof(daily -> 'staff_charge_rank') = 'number'
        AND (daily -> 'staff_spent') IS NOT NULL
        AND jsonb_typeof(daily -> 'staff_spent') = 'number'
        AND (daily -> 'drain_used') IS NOT NULL
        AND jsonb_typeof(daily -> 'drain_used') = 'boolean'
    );
