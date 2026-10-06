-- The user's Agree/Disagree on Shadow proposals, one row per message, so the
-- message list can join it. Replaces marks kept in the index store's kv table.
CREATE TABLE IF NOT EXISTS shadow_reviews (
    message_id TEXT PRIMARY KEY,
    review     TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
