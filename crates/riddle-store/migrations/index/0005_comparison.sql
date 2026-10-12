-- Model comparison (Developer → Models): every model's answer to every
-- comparison item, recorded and never applied. Escalation outputs are
-- keyed by message, Director outputs by the question replayed.
CREATE TABLE comparison_outputs (
  id TEXT PRIMARY KEY,
  task TEXT NOT NULL,
  source TEXT NOT NULL,
  model_id TEXT NOT NULL,
  dig_id TEXT,
  message_id TEXT,
  question_key TEXT,
  json TEXT NOT NULL,
  cost_usd REAL,
  created_at INTEGER NOT NULL
);
CREATE INDEX comparison_outputs_task ON comparison_outputs(task, created_at DESC);
CREATE INDEX comparison_outputs_message ON comparison_outputs(message_id);
