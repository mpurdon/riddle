-- Importance: every message is Low, Normal, High or Critical. The level of
-- a decision is a column so lists can filter and sort on it; the user's own
-- level for a message is a separate table, so a rule re-deciding the
-- message never overwrites it. Decisions made before this hid mail or not:
-- the hidden ones are Low, the rest Normal until Sift decides them again.
ALTER TABLE classifications ADD COLUMN importance INTEGER NOT NULL DEFAULT 1;
UPDATE classifications SET importance = 0, json = json_set(json, '$.importance', 'low')
  WHERE superseded_at IS NULL AND disposition_kind IN ('spoil', 'filed', 'contamination');
CREATE TABLE importance_overrides (
  message_id TEXT PRIMARY KEY,
  importance INTEGER NOT NULL,
  set_at INTEGER NOT NULL
);
