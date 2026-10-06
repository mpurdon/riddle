-- `+ Replaces` on Spoil and Filed: a store's sale mail where only the newest
-- matters. A decision made by such a route records its series (Mesh and
-- sender); after each one the series settles, and the older messages are
-- superseded by the newest, by message date. A row here is a message's
-- place in its series: `superseded_by` when a newer one made it moot,
-- `kept_at` when the user undid that and chose to keep it.
ALTER TABLE classifications ADD COLUMN series_key TEXT;
CREATE INDEX classifications_series ON classifications(series_key)
  WHERE series_key IS NOT NULL AND superseded_at IS NULL;
CREATE TABLE message_series (
  message_id TEXT PRIMARY KEY,
  series_key TEXT NOT NULL,
  superseded_by TEXT,
  kept_at INTEGER,
  updated_at INTEGER NOT NULL
);
CREATE INDEX message_series_key ON message_series(series_key);
