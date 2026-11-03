CREATE TABLE drafts (
  id uuid PRIMARY KEY,
  reader_id uuid NOT NULL REFERENCES book_smartz_web.accounts(reader_id),
  offer text NOT NULL,
  csrf text NOT NULL,
  browser_id uuid NOT NULL,
  session_hash bytea NOT NULL CHECK (octet_length(session_hash)=32),
  checking text,
  display_name text,
  biography text,
  CHECK ((display_name IS NULL) = (biography IS NULL))
);
CREATE INDEX reader_drafts ON drafts(reader_id);
CREATE INDEX browser_drafts ON drafts(browser_id);
CREATE TABLE login_returns (
  state_hash bytea PRIMARY KEY,
  draft_id uuid NOT NULL REFERENCES drafts(id) ON DELETE CASCADE
);
