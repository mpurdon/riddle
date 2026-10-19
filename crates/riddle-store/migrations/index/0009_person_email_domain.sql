-- The known-domain fact looked up `email LIKE '%@domain'`: a scan of every
-- address ever seen, once per message of a Sift pass (19 Oct 2026). A
-- generated domain column with an index makes it a point lookup; the
-- writers need not know it exists.
ALTER TABLE person_emails ADD COLUMN domain TEXT
  GENERATED ALWAYS AS (substr(email, instr(email, '@') + 1)) VIRTUAL;
CREATE INDEX person_emails_domain ON person_emails(domain);
