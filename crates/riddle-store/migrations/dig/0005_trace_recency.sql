-- The Inspector and the Observatories read the newest traces every few
-- seconds. Without this index each read sorted the whole table: 22,686
-- traces (258 MB) on a real mailbox took most of a second per read.
CREATE INDEX decision_traces_recent ON decision_traces(created_at DESC);
