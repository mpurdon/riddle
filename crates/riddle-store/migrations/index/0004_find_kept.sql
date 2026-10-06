-- A Find the user kept open after undoing a rule's closing of it
-- (`Find::kept_open_at`) is out of reach of every rule that closes Finds.
-- The column lets the queries that feed those rules (series settling,
-- Temporal Sift) leave kept Finds out, rather than each rule re-checking the
-- JSON, and stops Temporal Sift re-reading a kept Find past its date every
-- tick. Kept Finds already stored carry the time in their JSON.
ALTER TABLE finds ADD COLUMN kept_open_at INTEGER;
UPDATE finds SET kept_open_at =
  CAST((julianday(json_extract(json, '$.keptOpenAt')) - 2440587.5) * 86400000 AS INTEGER)
  WHERE json_extract(json, '$.keptOpenAt') IS NOT NULL;
