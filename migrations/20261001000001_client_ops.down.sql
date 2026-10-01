-- Reverse of 20261001000001_client_ops.sql, in reverse dependency order.
-- Losing the ledger costs idempotency for in-flight offline queues only; no
-- other table references it (specs/007-party-sync data-model.md).
DROP INDEX client_ops_account_sub_created_at_idx;
DROP TABLE client_ops;
