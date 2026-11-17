-- migration to add check_results table for persisting check outcomes
-- Version format: YYYYMMDDHHMMSS_check_results

CREATE TABLE IF NOT EXISTS check_results (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    target TEXT NOT NULL,
    protocol TEXT NOT NULL,
    success BOOLEAN NOT NULL,
    latency_ms INTEGER,
    error TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    consecutive_successes INTEGER NOT NULL DEFAULT 0,
    last_state_change BOOLEAN NOT NULL DEFAULT 0,
    previous_state BOOLEAN,
    observed_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_check_results_entity ON check_results(entity_id, entity_type);
CREATE INDEX IF NOT EXISTS idx_check_results_observed_at ON check_results(observed_at);
CREATE INDEX IF NOT EXISTS idx_check_results_protocol ON check_results(protocol);
CREATE INDEX IF NOT EXISTS idx_check_results_success ON check_results(success);