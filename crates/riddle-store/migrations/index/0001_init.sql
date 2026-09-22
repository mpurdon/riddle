-- Index store: Digs, policy (Signals, Meshes, Strata), cross-Dig entities
-- (Finds, Activity, jobs, People, Expeditions), the User Profile, pending
-- actions and idempotency keys.

CREATE TABLE digs (
  id TEXT PRIMARY KEY,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE dig_groups (
  name TEXT PRIMARY KEY,
  json TEXT NOT NULL
);

CREATE TABLE signals (
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  version INTEGER NOT NULL,
  status TEXT NOT NULL,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (name, version)
);

CREATE TABLE meshes (
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  version INTEGER NOT NULL,
  status TEXT NOT NULL,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  deleted_at INTEGER,
  PRIMARY KEY (id, version)
);
CREATE INDEX meshes_name ON meshes(name);

CREATE TABLE strata (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  json TEXT NOT NULL,
  deleted_at INTEGER
);

CREATE TABLE finds (
  id TEXT PRIMARY KEY,
  find_type TEXT NOT NULL,
  status TEXT NOT NULL,
  priority INTEGER NOT NULL,
  identity_key TEXT NOT NULL,
  due_at INTEGER,
  snoozed_until INTEGER,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX finds_status ON finds(status, priority DESC);
CREATE INDEX finds_identity ON finds(identity_key);

CREATE TABLE find_sources (
  find_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  dig_id TEXT NOT NULL,
  PRIMARY KEY (find_id, message_id)
);
CREATE INDEX find_sources_message ON find_sources(message_id);

CREATE TABLE activities (
  id TEXT PRIMARY KEY,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  dig_id TEXT,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX activities_created ON activities(created_at DESC);

CREATE TABLE jobs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  priority INTEGER NOT NULL,
  status TEXT NOT NULL,
  dig_id TEXT,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX jobs_status ON jobs(status, priority, created_at);

CREATE TABLE expeditions (
  id TEXT PRIMARY KEY,
  status TEXT NOT NULL,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE people (
  id TEXT PRIMARY KEY,
  json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE person_emails (
  email TEXT PRIMARY KEY,
  person_id TEXT NOT NULL
);

CREATE TABLE user_profile (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  json TEXT NOT NULL
);

CREATE TABLE pending_actions (
  id TEXT PRIMARY KEY,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE idempotency (
  key TEXT PRIMARY KEY,
  action TEXT NOT NULL,
  result_json TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE kv (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE conversations (
  id TEXT PRIMARY KEY,
  json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE experiments (
  id TEXT PRIMARY KEY,
  json TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE audit_samples (
  id TEXT PRIMARY KEY,
  signal_name TEXT NOT NULL,
  dig_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  label TEXT,
  created_at INTEGER NOT NULL
);
