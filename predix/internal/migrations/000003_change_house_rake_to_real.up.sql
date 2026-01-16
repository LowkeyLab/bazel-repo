ALTER TABLE contests ALTER COLUMN house_rake TYPE DOUBLE PRECISION USING (house_rake::numeric / 100.0);
ALTER TABLE contests ALTER COLUMN house_rake SET DEFAULT 0.10;
