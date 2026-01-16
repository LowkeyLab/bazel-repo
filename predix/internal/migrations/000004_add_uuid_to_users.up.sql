CREATE EXTENSION IF NOT EXISTS "pgcrypto";

ALTER TABLE users ADD COLUMN uuid UUID DEFAULT gen_random_uuid();
-- Backfill not strictly needed due to DEFAULT but good for explicit intent if default wasn't there
UPDATE users SET uuid = gen_random_uuid() WHERE uuid IS NULL;
ALTER TABLE users ALTER COLUMN uuid SET NOT NULL;
ALTER TABLE users ADD CONSTRAINT users_uuid_key UNIQUE (uuid);

-- Circles
ALTER TABLE circles ADD COLUMN creator_uuid UUID;
UPDATE circles c SET creator_uuid = u.uuid FROM users u WHERE c.creator_id = u.id;
ALTER TABLE circles ALTER COLUMN creator_uuid SET NOT NULL;
ALTER TABLE circles DROP COLUMN creator_id;
ALTER TABLE circles RENAME COLUMN creator_uuid TO creator_id;
ALTER TABLE circles ADD CONSTRAINT circles_creator_id_fkey FOREIGN KEY (creator_id) REFERENCES users(uuid);

-- Circle Members
ALTER TABLE circle_members ADD COLUMN user_uuid UUID;
UPDATE circle_members cm SET user_uuid = u.uuid FROM users u WHERE cm.user_id = u.id;
ALTER TABLE circle_members ALTER COLUMN user_uuid SET NOT NULL;
-- Drop PK constraint (name usually circle_members_pkey)
ALTER TABLE circle_members DROP CONSTRAINT circle_members_pkey;
ALTER TABLE circle_members DROP COLUMN user_id;
ALTER TABLE circle_members RENAME COLUMN user_uuid TO user_id;
ALTER TABLE circle_members ADD CONSTRAINT circle_members_pkey PRIMARY KEY (circle_id, user_id);
ALTER TABLE circle_members ADD CONSTRAINT circle_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(uuid);

-- Contests
ALTER TABLE contests ADD COLUMN creator_uuid UUID;
UPDATE contests c SET creator_uuid = u.uuid FROM users u WHERE c.creator_id = u.id;
ALTER TABLE contests ALTER COLUMN creator_uuid SET NOT NULL;
ALTER TABLE contests DROP COLUMN creator_id;
ALTER TABLE contests RENAME COLUMN creator_uuid TO creator_id;
ALTER TABLE contests ADD CONSTRAINT contests_creator_id_fkey FOREIGN KEY (creator_id) REFERENCES users(uuid);

-- Predictions
ALTER TABLE predictions ADD COLUMN user_uuid UUID;
UPDATE predictions p SET user_uuid = u.uuid FROM users u WHERE p.user_id = u.id;
ALTER TABLE predictions ALTER COLUMN user_uuid SET NOT NULL;
-- Drop PK constraint (name usually predictions_pkey)
ALTER TABLE predictions DROP CONSTRAINT predictions_pkey;
ALTER TABLE predictions DROP COLUMN user_id;
ALTER TABLE predictions RENAME COLUMN user_uuid TO user_id;
ALTER TABLE predictions ADD CONSTRAINT predictions_pkey PRIMARY KEY (contest_id, user_id, option_id);
ALTER TABLE predictions ADD CONSTRAINT predictions_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(uuid);
