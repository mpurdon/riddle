-- Activity names the Director conversation behind it, so one conversation's
-- proposals, previews and the changes applied from them can be read together.
ALTER TABLE activities ADD COLUMN conversation_id TEXT;
CREATE INDEX activities_conversation ON activities(conversation_id, created_at DESC);
