ALTER TABLE contests ADD COLUMN expires_at TIMESTAMP;

-- Backfill existing contests: Expires 7 days after ClosesAt
UPDATE contests SET expires_at = locked_at + INTERVAL '7 days';

ALTER TABLE contests ALTER COLUMN expires_at SET NOT NULL;
