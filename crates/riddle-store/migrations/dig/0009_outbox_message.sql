-- `recently_applied_for_message` filtered the outbox by message with only
-- the pending index to help: a scan of every row ever applied, once per
-- external label change the sync saw (19 Oct 2026).
CREATE INDEX outbox_message_applied ON outbox(message_id, applied_at);
