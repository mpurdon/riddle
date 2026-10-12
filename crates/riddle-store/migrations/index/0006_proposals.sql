-- Every proposal and what became of it (applied, declined, replaced,
-- expired, failed). pending_actions holds only the open ones and drops a row
-- when it resolves, so a declined Director draft could not be read again.
CREATE TABLE proposals (
  id TEXT PRIMARY KEY,
  conversation_id TEXT,
  action TEXT NOT NULL,
  outcome TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  resolved_at INTEGER,
  json TEXT NOT NULL
);
CREATE INDEX proposals_conversation ON proposals(conversation_id, created_at DESC);
CREATE INDEX proposals_recent ON proposals(created_at DESC);
