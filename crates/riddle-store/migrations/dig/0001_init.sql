-- Per-Dig store: provider mail, facts, evaluations, classifications, traces.
-- Riddle policy (Signals, Meshes, Strata) and cross-Dig entities (Finds,
-- Activity, jobs) live in the index store. Data never crosses Dig boundaries
-- by accident because each Dig is its own file (ARCHITECTURE.md §11).

CREATE TABLE messages (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL UNIQUE,
  thread_id TEXT NOT NULL,
  subject TEXT NOT NULL,
  from_email TEXT NOT NULL,
  from_name TEXT,
  to_json TEXT NOT NULL,
  cc_json TEXT NOT NULL,
  bcc_json TEXT NOT NULL,
  date INTEGER NOT NULL,
  snippet TEXT NOT NULL,
  labels_json TEXT NOT NULL,
  headers_json TEXT NOT NULL,
  attachments_json TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  is_sent INTEGER NOT NULL,
  body_text TEXT,
  body_html TEXT,
  synced_at INTEGER NOT NULL,
  deleted_at INTEGER
);
CREATE INDEX messages_thread ON messages(thread_id);
CREATE INDEX messages_date ON messages(date DESC);
CREATE INDEX messages_from ON messages(from_email);

CREATE VIRTUAL TABLE messages_fts USING fts5(
  subject, body_text, from_email,
  content='messages', content_rowid='rowid',
  tokenize='porter unicode61'
);
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, subject, body_text, from_email)
  VALUES (new.rowid, new.subject, new.body_text, new.from_email);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, subject, body_text, from_email)
  VALUES ('delete', old.rowid, old.subject, old.body_text, old.from_email);
END;
CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, subject, body_text, from_email)
  VALUES ('delete', old.rowid, old.subject, old.body_text, old.from_email);
  INSERT INTO messages_fts(rowid, subject, body_text, from_email)
  VALUES (new.rowid, new.subject, new.body_text, new.from_email);
END;

CREATE TABLE threads (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL UNIQUE,
  subject TEXT NOT NULL,
  last_message_at INTEGER,
  message_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE thread_state (
  thread_id TEXT PRIMARY KEY,
  json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE facts (
  message_id TEXT PRIMARY KEY,
  json TEXT NOT NULL,
  computed_at INTEGER NOT NULL
);

CREATE TABLE signal_evaluations (
  message_id TEXT NOT NULL,
  signal_name TEXT NOT NULL,
  signal_version INTEGER NOT NULL,
  engine_identity TEXT NOT NULL,
  context_hash TEXT NOT NULL DEFAULT '',
  json TEXT NOT NULL,
  evaluated_at INTEGER NOT NULL,
  PRIMARY KEY (message_id, signal_name, signal_version, engine_identity, context_hash)
);

CREATE TABLE classifications (
  id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL,
  disposition_kind TEXT NOT NULL,
  find_type TEXT,
  mesh_id TEXT,
  mesh_version INTEGER,
  json TEXT NOT NULL,
  shadow INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  superseded_at INTEGER
);
CREATE INDEX classifications_message ON classifications(message_id, superseded_at);
CREATE INDEX classifications_current ON classifications(disposition_kind, shadow) WHERE superseded_at IS NULL;

CREATE TABLE message_strata (
  message_id TEXT NOT NULL,
  stratum TEXT NOT NULL,
  PRIMARY KEY (message_id, stratum)
);

CREATE TABLE decision_traces (
  id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX decision_traces_message ON decision_traces(message_id, created_at DESC);

CREATE TABLE corrections (
  id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  retracted_at INTEGER
);

CREATE TABLE sync_state (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE outbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  thread_id TEXT,
  message_id TEXT,
  op TEXT NOT NULL,
  json TEXT NOT NULL,
  idempotency_key TEXT UNIQUE,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  applied_at INTEGER,
  error TEXT
);
CREATE INDEX outbox_pending ON outbox(next_attempt_at) WHERE applied_at IS NULL;

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

CREATE TABLE snoozes (
  message_id TEXT PRIMARY KEY,
  until INTEGER NOT NULL
);

CREATE TABLE drafts (
  id TEXT PRIMARY KEY,
  json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE labels (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  managed INTEGER NOT NULL,
  system INTEGER NOT NULL
);
