-- Two Mesh route outputs let a Find stop mattering (§41).
--
-- `+ Replaces` gives a Find a series key (Mesh and sender); the newest Find
-- of a series supersedes the others, which needs the open members of a
-- series found without scanning JSON.
--
-- `+ Expires:` gives any Find type an expiry, kept apart from due_at, which
-- is a deadline that raises priority rather than a time after which the
-- Find stops mattering. Temporal Sift reads it by status first, so the index
-- leads with status; on expires_at alone the planner prefers finds_status.
ALTER TABLE finds ADD COLUMN series_key TEXT;
CREATE INDEX finds_series ON finds(series_key) WHERE series_key IS NOT NULL;
ALTER TABLE finds ADD COLUMN expires_at INTEGER;
CREATE INDEX finds_expiry ON finds(status, expires_at) WHERE expires_at IS NOT NULL;
