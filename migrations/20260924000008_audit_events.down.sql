-- Reverse of 20260924000008_audit_events.sql, in reverse dependency order.
DROP INDEX audit_events_actor_sub_idx;
DROP INDEX audit_events_occurred_at_idx;
DROP TABLE audit_events;
