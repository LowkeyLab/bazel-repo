CREATE TABLE search_gate (
  singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
  next_search_at timestamptz NOT NULL
);
INSERT INTO search_gate (singleton, next_search_at) VALUES (true, '-infinity');
CREATE TABLE search_admissions (
  reader_id uuid NOT NULL REFERENCES book_smartz_web.accounts(reader_id),
  admitted_at timestamptz NOT NULL
);
CREATE INDEX search_admissions_reader ON search_admissions (reader_id);
CREATE INDEX search_admissions_time ON search_admissions (admitted_at);
