CREATE TABLE profiles (
  reader_id uuid PRIMARY KEY REFERENCES book_smartz_web.accounts(reader_id),
  display_name text NOT NULL DEFAULT '',
  biography text NOT NULL DEFAULT ''
);
