-- Markers the Sift backlog, the body backfill and Temporal Sift read from
-- the rows themselves, so an idle pass is an index probe, not a scan.
-- Every pass (45 s) `unclassified_ids` walked the date index from the
-- newest and probed `classifications` per row until it reached the unsifted
-- ones (the oldest, since Sift runs newest first), `unclassified_count` and
-- `count_without_body` read every message, and `with_open_asks_from_me`
-- deserialised every thread's JSON (17 Oct 2026).
--
-- `sifted_at` is the created_at of the message's current live
-- classification and `shadow_sifted_at` that of its current Shadow
-- proposal; NULL means none. The triggers below keep both equal to the
-- classifications table on every insert, supersede or delete, in the same
-- transaction, so no writer has to remember.

ALTER TABLE messages ADD COLUMN sifted_at INTEGER;
ALTER TABLE messages ADD COLUMN shadow_sifted_at INTEGER;

-- The FTS mirror only needs to hear about the columns it indexes. Until now
-- it re-tokenised a message's body on every update of the row, labels and
-- deletions included, and would have on every decision below.
DROP TRIGGER messages_au;
CREATE TRIGGER messages_au AFTER UPDATE OF subject, body_text, from_email ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, subject, body_text, from_email)
  VALUES ('delete', old.rowid, old.subject, old.body_text, old.from_email);
  INSERT INTO messages_fts(rowid, subject, body_text, from_email)
  VALUES (new.rowid, new.subject, new.body_text, new.from_email);
END;

UPDATE messages SET
  sifted_at = (SELECT max(k.created_at) FROM classifications k
               WHERE k.message_id = messages.id AND k.shadow = 0 AND k.superseded_at IS NULL),
  shadow_sifted_at = (SELECT max(k.created_at) FROM classifications k
                      WHERE k.message_id = messages.id AND k.shadow = 1 AND k.superseded_at IS NULL)
WHERE id IN (SELECT message_id FROM classifications WHERE superseded_at IS NULL);

CREATE INDEX messages_unsifted ON messages(date DESC)
  WHERE sifted_at IS NULL AND deleted_at IS NULL;
CREATE INDEX messages_without_body ON messages(date DESC)
  WHERE body_text IS NULL AND body_html IS NULL AND deleted_at IS NULL;

CREATE TRIGGER classifications_ai_sifted AFTER INSERT ON classifications BEGIN
  UPDATE messages SET
    sifted_at = (SELECT max(created_at) FROM classifications
                 WHERE message_id = new.message_id AND shadow = 0 AND superseded_at IS NULL),
    shadow_sifted_at = (SELECT max(created_at) FROM classifications
                        WHERE message_id = new.message_id AND shadow = 1 AND superseded_at IS NULL)
  WHERE id = new.message_id;
END;
CREATE TRIGGER classifications_au_sifted AFTER UPDATE OF superseded_at ON classifications BEGIN
  UPDATE messages SET
    sifted_at = (SELECT max(created_at) FROM classifications
                 WHERE message_id = new.message_id AND shadow = 0 AND superseded_at IS NULL),
    shadow_sifted_at = (SELECT max(created_at) FROM classifications
                        WHERE message_id = new.message_id AND shadow = 1 AND superseded_at IS NULL)
  WHERE id = new.message_id;
END;
CREATE TRIGGER classifications_ad_sifted AFTER DELETE ON classifications BEGIN
  UPDATE messages SET
    sifted_at = (SELECT max(created_at) FROM classifications
                 WHERE message_id = old.message_id AND shadow = 0 AND superseded_at IS NULL),
    shadow_sifted_at = (SELECT max(created_at) FROM classifications
                        WHERE message_id = old.message_id AND shadow = 1 AND superseded_at IS NULL)
  WHERE id = old.message_id;
END;

-- Sender addresses are stored lowercase (`Address::new` lowercases and the
-- row writer now does too), so the sender queries drop lower() and use
-- messages_from.
UPDATE messages SET from_email = lower(from_email) WHERE from_email <> lower(from_email);

-- Thread State: whether I have an open ask, as a column, so the Follow Up
-- scan reads the few threads that matter.
ALTER TABLE thread_state ADD COLUMN has_open_asks INTEGER NOT NULL DEFAULT 0;
UPDATE thread_state SET has_open_asks = 1 WHERE json_array_length(json, '$.openAsksFromMe') > 0;
CREATE INDEX thread_state_open_asks ON thread_state(has_open_asks) WHERE has_open_asks = 1;
