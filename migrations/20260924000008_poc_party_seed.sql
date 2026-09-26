-- E5 raised finding 1/2 (design §5.1, gate-approved): the single POC party.
--
-- characters.party_id is NOT NULL and E2 seeds nothing — without a party
-- row the first import cannot insert. A second party is an INSERT, never a
-- migration (E2); this seeds exactly the one POC party, and only when the
-- table is empty, so an operator-created party is never overwritten.
-- Idempotent under re-run by the same guard.

INSERT INTO parties (name)
SELECT 'POC Party'
WHERE NOT EXISTS (SELECT 1 FROM parties);
