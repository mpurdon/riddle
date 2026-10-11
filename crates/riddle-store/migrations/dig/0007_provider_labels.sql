-- The account's labels by id and name. Messages carry Gmail's label ids
-- verbatim (`Label_4452336792128208639`), and nothing turned a user's own
-- label into its name, so the app could not show the labels an account has.
CREATE TABLE provider_labels (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  system INTEGER NOT NULL,
  managed INTEGER NOT NULL,
  synced_at INTEGER NOT NULL
);
