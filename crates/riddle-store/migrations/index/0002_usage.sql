-- Usage rows that belong to no single Dig (Director and Expedition calls
-- spanning Digs, probes) live in the index store; per-Dig rows stay in the
-- Dig store. Same columns so one repository serves both.
CREATE TABLE usage (
  id TEXT PRIMARY KEY,
  at INTEGER NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  task TEXT NOT NULL,
  trace_id TEXT,
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  cost_usd REAL,
  latency_ms INTEGER NOT NULL,
  ok INTEGER NOT NULL
);
CREATE INDEX usage_at ON usage(at DESC);
