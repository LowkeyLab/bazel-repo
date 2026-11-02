ALTER TABLE drafts ADD COLUMN expires_at timestamptz NOT NULL DEFAULT (now()+interval '720 hours');
UPDATE drafts d SET expires_at=s.expires_at+interval '720 hours'
FROM book_smartz_web.sessions s WHERE s.token_hash=d.session_hash;
CREATE INDEX expired_drafts ON drafts(expires_at);
ALTER TABLE login_returns ADD COLUMN expires_at timestamptz NOT NULL DEFAULT (now()+interval '1 hour');
CREATE INDEX expired_login_returns ON login_returns(expires_at);
