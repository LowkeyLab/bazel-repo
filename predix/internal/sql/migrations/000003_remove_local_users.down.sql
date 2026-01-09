-- This is a simplified down migration, might not perfectly restore data but restores schema structure.
CREATE TYPE user_role AS ENUM ('member', 'admin');
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    authorizer_id TEXT UNIQUE,
    role user_role NOT NULL DEFAULT 'member'
);

-- We can't easily convert TEXT back to INT IDs without knowing the mapping, 
-- so we'll just change the types back and they might be empty or invalid.
ALTER TABLE circles ALTER COLUMN creator_id TYPE INT USING 0;
ALTER TABLE circle_members ALTER COLUMN user_id TYPE INT USING 0;
ALTER TABLE contests ALTER COLUMN creator_id TYPE INT USING 0;
ALTER TABLE predictions ALTER COLUMN user_id TYPE INT USING 0;

ALTER TABLE circles ADD CONSTRAINT circles_creator_id_fkey FOREIGN KEY (creator_id) REFERENCES users(id);
ALTER TABLE circle_members ADD CONSTRAINT circle_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);
ALTER TABLE contests ADD CONSTRAINT contests_creator_id_fkey FOREIGN KEY (creator_id) REFERENCES users(id);
ALTER TABLE predictions ADD CONSTRAINT predictions_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);
