ALTER TABLE users ADD COLUMN authorizer_id TEXT UNIQUE;
ALTER TABLE users DROP COLUMN password_hash;
