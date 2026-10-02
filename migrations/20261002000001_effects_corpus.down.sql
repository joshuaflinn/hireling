-- E8 migration 1/1 down: drop the corpus-linked effect columns.
ALTER TABLE effects DROP COLUMN corpus_entry_id;
ALTER TABLE effects DROP COLUMN tracked_manually;
