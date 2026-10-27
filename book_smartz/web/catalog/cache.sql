CREATE TABLE works (
  work_id text PRIMARY KEY CHECK (work_id ~ '^OL[1-9][0-9]*W$' AND length(work_id) <= 64),
  metadata jsonb NOT NULL,
  source text NOT NULL,
  fetched_at timestamptz NOT NULL,
  fresh_until timestamptz NOT NULL,
  last_used_at timestamptz NOT NULL
);
CREATE INDEX works_last_use ON works (last_used_at, work_id);
