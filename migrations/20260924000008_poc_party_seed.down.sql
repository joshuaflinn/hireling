-- Down for 20260924000008_poc_party_seed: remove the seeded POC party only
-- if it is still untouched (default name) and unoccupied by any character.

DELETE FROM parties
WHERE name = 'POC Party'
  AND quartermaster_character_id IS NULL
  AND NOT EXISTS (
      SELECT 1 FROM characters WHERE characters.party_id = parties.id
  );
