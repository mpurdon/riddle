-- A proposal's outcome lives in columns, and `json` holds the proposal
-- alone: the outcome used to sit in both the JSON and the columns, so
-- recording it meant reading, parsing and rewriting the row.
ALTER TABLE proposals ADD COLUMN note TEXT;
ALTER TABLE proposals ADD COLUMN activity_id TEXT;
