ALTER TABLE contests ALTER COLUMN house_rake TYPE DECIMAL(5, 2) USING (house_rake * 100.0);
ALTER TABLE contests ALTER COLUMN house_rake SET DEFAULT 10.00;
