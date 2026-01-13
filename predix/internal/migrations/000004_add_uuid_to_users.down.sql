-- Predictions
ALTER TABLE predictions ADD COLUMN user_id_int INT;
UPDATE predictions p SET user_id_int = u.id FROM users u WHERE p.user_id = u.uuid;
ALTER TABLE predictions ALTER COLUMN user_id_int SET NOT NULL;
ALTER TABLE predictions DROP CONSTRAINT predictions_pkey;
ALTER TABLE predictions DROP COLUMN user_id;
ALTER TABLE predictions RENAME COLUMN user_id_int TO user_id;
ALTER TABLE predictions ADD CONSTRAINT predictions_pkey PRIMARY KEY (contest_id, user_id, option_id);
ALTER TABLE predictions ADD CONSTRAINT predictions_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);

-- Contests
ALTER TABLE contests ADD COLUMN creator_id_int INT;
UPDATE contests c SET creator_id_int = u.id FROM users u WHERE c.creator_id = u.uuid;
ALTER TABLE contests ALTER COLUMN creator_id_int SET NOT NULL;
ALTER TABLE contests DROP COLUMN creator_id;
ALTER TABLE contests RENAME COLUMN creator_id_int TO creator_id;
ALTER TABLE contests ADD CONSTRAINT contests_creator_id_fkey FOREIGN KEY (creator_id) REFERENCES users(id);

-- Circle Members
ALTER TABLE circle_members ADD COLUMN user_id_int INT;
UPDATE circle_members cm SET user_id_int = u.id FROM users u WHERE cm.user_id = u.uuid;
ALTER TABLE circle_members ALTER COLUMN user_id_int SET NOT NULL;
ALTER TABLE circle_members DROP CONSTRAINT circle_members_pkey;
ALTER TABLE circle_members DROP COLUMN user_id;
ALTER TABLE circle_members RENAME COLUMN user_id_int TO user_id;
ALTER TABLE circle_members ADD CONSTRAINT circle_members_pkey PRIMARY KEY (circle_id, user_id);
ALTER TABLE circle_members ADD CONSTRAINT circle_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);

-- Circles
ALTER TABLE circles ADD COLUMN creator_id_int INT;
UPDATE circles c SET creator_id_int = u.id FROM users u WHERE c.creator_id = u.uuid;
ALTER TABLE circles ALTER COLUMN creator_id_int SET NOT NULL;
ALTER TABLE circles DROP COLUMN creator_id;
ALTER TABLE circles RENAME COLUMN creator_id_int TO creator_id;
ALTER TABLE circles ADD CONSTRAINT circles_creator_id_fkey FOREIGN KEY (creator_id) REFERENCES users(id);

-- Users
ALTER TABLE users DROP COLUMN uuid;
