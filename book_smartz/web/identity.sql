CREATE TABLE accounts (
  issuer text NOT NULL,
  subject text NOT NULL,
  reader_id uuid NOT NULL UNIQUE,
  PRIMARY KEY (issuer, subject)
);
CREATE TABLE sessions (
  token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
  reader_id uuid NOT NULL REFERENCES accounts(reader_id),
  csrf text NOT NULL,
  expires_at timestamptz NOT NULL
);
CREATE TABLE login_challenges (
  state_hash bytea PRIMARY KEY CHECK (octet_length(state_hash) = 32),
  nonce text NOT NULL,
  verifier text NOT NULL,
  expires_at timestamptz NOT NULL
);
