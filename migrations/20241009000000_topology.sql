-- migration to add topology tables
-- Version format: YYYYMMDDHHMMSS_topology

-- Drop the old topology_edges table from initial migration if it exists
DROP TABLE IF EXISTS topology_edges;

CREATE TABLE IF NOT EXISTS topology_nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    label TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}',
    state TEXT NOT NULL DEFAULT 'unknown',
    observed_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS topology_edges (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    provenance TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.5,
    observed_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(source) REFERENCES topology_nodes(id) ON DELETE CASCADE,
    FOREIGN KEY(target) REFERENCES topology_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS topology_snapshots (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    nodes_json TEXT NOT NULL,
    edges_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for topology tables
CREATE INDEX IF NOT EXISTS idx_topology_nodes_type ON topology_nodes(node_type);
CREATE INDEX IF NOT EXISTS idx_topology_nodes_state ON topology_nodes(state);
CREATE INDEX IF NOT EXISTS idx_topology_edges_source ON topology_edges(source);
CREATE INDEX IF NOT EXISTS idx_topology_edges_target ON topology_edges(target);
CREATE INDEX IF NOT EXISTS idx_topology_snapshots_timestamp ON topology_snapshots(timestamp);
