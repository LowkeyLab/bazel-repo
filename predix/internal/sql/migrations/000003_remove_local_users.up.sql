-- Remove foreign keys
ALTER TABLE circles DROP CONSTRAINT IF EXISTS circles_creator_id_fkey;
ALTER TABLE circle_members DROP CONSTRAINT IF EXISTS circle_members_user_id_fkey;
ALTER TABLE contests DROP CONSTRAINT IF EXISTS contests_creator_id_fkey;
ALTER TABLE predictions DROP CONSTRAINT IF EXISTS predictions_user_id_fkey;

-- Change types to TEXT (authorizer_id)
ALTER TABLE circles ALTER COLUMN creator_id TYPE TEXT;
ALTER TABLE circle_members ALTER COLUMN user_id TYPE TEXT;
ALTER TABLE contests ALTER COLUMN creator_id TYPE TEXT;
ALTER TABLE predictions ALTER COLUMN user_id TYPE TEXT;

-- Drop users table and associated type
DROP TABLE IF EXISTS users;
DROP TYPE IF EXISTS user_role;
