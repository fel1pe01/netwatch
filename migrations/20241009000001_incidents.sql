-- migration to add incidents table
-- Version format: YYYYMMDDHHMMSS_incidents

CREATE TABLE IF NOT EXISTS incidents (
    id TEXT PRIMARY KEY,
    dedup_key TEXT NOT NULL,
    correlation_id TEXT,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    state TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    evidence TEXT NOT NULL DEFAULT '[]',
    labels TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    acknowledged_at TEXT,
    acknowledged_by TEXT,
    resolved_at TEXT,
    resolved_by TEXT,
    closed_at TEXT,
    transitions TEXT NOT NULL DEFAULT '[]',
    cooldown_until TEXT,
    silence_until TEXT,
    root_cause_hypothesis_id TEXT,
    root_cause_confirmed BOOLEAN NOT NULL DEFAULT 0,
    root_cause_confirmed_at TEXT,
    root_cause_confirmed_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_incidents_dedup_key ON incidents(dedup_key);
CREATE INDEX IF NOT EXISTS idx_incidents_entity ON incidents(entity_id, entity_type);
CREATE INDEX IF NOT EXISTS idx_incidents_state ON incidents(state);
CREATE INDEX IF NOT EXISTS idx_incidents_hypothesis ON incidents(root_cause_hypothesis_id);