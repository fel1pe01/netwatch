-- migration to add events table with schema v2 (matching Event struct)
-- Version format: YYYYMMDDHHMMSS_events_v2

CREATE TABLE IF NOT EXISTS events_v2 (
    id TEXT PRIMARY KEY,
    schema_version INTEGER NOT NULL DEFAULT 2,
    event_type TEXT NOT NULL,
    event_ts TEXT NOT NULL,
    observed_ts TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    state TEXT NOT NULL,
    source TEXT NOT NULL,
    evidence TEXT NOT NULL DEFAULT '{}',
    dedup_key TEXT NOT NULL,
    correlation_id TEXT,
    trace_id TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_events_v2_entity ON events_v2(entity_id, entity_type);
CREATE INDEX IF NOT EXISTS idx_events_v2_event_ts ON events_v2(event_ts);
CREATE INDEX IF NOT EXISTS idx_events_v2_event_type ON events_v2(event_type);
CREATE INDEX IF NOT EXISTS idx_events_v2_dedup_key ON events_v2(dedup_key);
CREATE INDEX IF NOT EXISTS idx_events_v2_correlation_id ON events_v2(correlation_id);