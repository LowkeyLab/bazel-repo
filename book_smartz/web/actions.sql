CREATE TABLE display (
  book_id uuid PRIMARY KEY REFERENCES book_smartz.books(id),
  title text NOT NULL CHECK (length(btrim(title)) > 0),
  authors text[] NOT NULL,
  cover bigint CHECK (cover > 0),
  source text NOT NULL
);
