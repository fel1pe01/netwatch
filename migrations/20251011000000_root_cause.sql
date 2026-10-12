-- migration to add root cause analysis fields to incidents (if not already present)
-- Version format: YYYYMMDDHHMMSS_root_cause
-- Note: The incidents table is created in 20241009000001_incidents.sql with these columns already.
-- This migration is kept for versioning but does not alter the table.

-- If the incidents table was created before this migration without root cause columns,
-- uncomment the following:
-- ALTER TABLE incidents ADD COLUMN root_cause_hypothesis_id TEXT;
-- ALTER TABLE incidents ADD COLUMN root_cause_confirmed BOOLEAN NOT NULL DEFAULT 0;
-- ALTER TABLE incidents ADD COLUMN root_cause_confirmed_at TEXT;
-- ALTER TABLE incidents ADD COLUMN root_cause_confirmed_by TEXT;

-- Index for hypothesis lookups
CREATE INDEX IF NOT EXISTS idx_incidents_hypothesis ON incidents(root_cause_hypothesis_id);