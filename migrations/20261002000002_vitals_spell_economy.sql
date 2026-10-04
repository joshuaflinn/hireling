-- E6 migration: the Q1-ruling spell-economy vitals fields (design §3).
--
-- Three new tracked vitals fields on character_vitals, exactly the E2
-- pattern: a value column with its bounds CHECK, and its own *_version
-- column fed by the one global field_version_seq (per-field CAS — focus
-- pips and hero points move independently mid-fight, so they version
-- independently; design §11 decision 1).
--
-- `daily` is one whole-row field (staff_charge_rank, staff_spent,
-- drain_used): the three values reset together on New Day and are read
-- together; splitting them buys three versions for values that never race
-- (design §11 decision 2). The client clamps focus to the character's
-- focus max; the server validates >= 0 only (wire-protocol §3).

ALTER TABLE character_vitals
    ADD COLUMN focus_current int NOT NULL DEFAULT 0
        CHECK (focus_current >= 0),
    ADD COLUMN focus_version bigint NOT NULL DEFAULT nextval('field_version_seq'),
    ADD COLUMN hero_points int NOT NULL DEFAULT 0
        CHECK (hero_points >= 0),
    ADD COLUMN hero_points_version bigint NOT NULL DEFAULT nextval('field_version_seq'),
    ADD COLUMN daily jsonb NOT NULL DEFAULT '{"staff_charge_rank":0,"staff_spent":0,"drain_used":false}'::jsonb,
    ADD COLUMN daily_version bigint NOT NULL DEFAULT nextval('field_version_seq');
