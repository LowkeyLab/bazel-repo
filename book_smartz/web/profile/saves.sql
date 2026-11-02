ALTER TABLE book_smartz_profiles.profiles
  ADD COLUMN revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
  ADD COLUMN import_attempted boolean NOT NULL DEFAULT false;
CREATE TABLE submissions (
  reader_id uuid NOT NULL REFERENCES book_smartz_web.accounts(reader_id),
  submission_id uuid NOT NULL,
  revision bigint NOT NULL CHECK (revision >= 0),
  display_name text,
  biography text,
  outcome text NOT NULL CHECK (outcome IN ('pending', 'saved', 'failed', 'conflict')),
  acknowledged boolean NOT NULL DEFAULT false,
  PRIMARY KEY (reader_id, submission_id),
  CHECK ((display_name IS NULL) = (biography IS NULL)),
  CHECK (outcome = 'failed' OR display_name IS NOT NULL)
);
CREATE UNIQUE INDEX one_unconfirmed_profile_submission
  ON submissions(reader_id) WHERE NOT acknowledged;
