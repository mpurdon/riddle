-- Local semantic index (ARCHITECTURE.md §45): one vector per message, built
-- on device by riddle-embeddings. `model` names the embedding scheme so a
-- real model can replace the hashed bag-of-words without a schema change;
-- rows with an old model are simply re-embedded.

CREATE TABLE embeddings (
  message_id TEXT PRIMARY KEY,
  model TEXT NOT NULL,
  dim INTEGER NOT NULL,
  vector BLOB NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX embeddings_model ON embeddings(model);
