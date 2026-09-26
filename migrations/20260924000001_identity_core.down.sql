-- Reverse of 20260924000001_identity_core.sql, in reverse dependency order.
ALTER TABLE parties DROP CONSTRAINT parties_quartermaster_character_id_fkey;
DROP INDEX characters_party_id_idx;
DROP TABLE characters;
DROP TABLE parties;
DROP TABLE accounts;
DROP SEQUENCE field_version_seq;
