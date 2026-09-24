-- Reverse of 20260924000005_shared_inventory.sql, in reverse dependency order.
DROP TRIGGER claim_history_append_only ON claim_history;
DROP FUNCTION forbid_claim_history_mutation;
DROP TABLE party_bank;
DROP INDEX claim_history_actor_sub_idx;
DROP INDEX claim_history_party_id_idx;
DROP TABLE claim_history;
DROP INDEX party_stash_party_id_idx;
DROP TABLE party_stash;
