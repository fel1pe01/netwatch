-- migration to deduplicate devices by IP, then add unique indexes, routes table, and last_seen columns
-- Version format: YYYYMMDDHHMMSS_inventory_dedup_and_indexes

-- ========================================================================
-- PART 1: DEDUPLICATION (must run BEFORE unique indexes)
-- ========================================================================

-- Step 1: Create a temp table with the keeper device for each IP (most recent updated_at)
CREATE TEMP TABLE device_keepers AS
WITH ranked AS (
    SELECT
        id,
        ip,
        ROW_NUMBER() OVER (PARTITION BY ip ORDER BY updated_at DESC) as rn
    FROM devices
    WHERE ip IS NOT NULL AND ip != ''
)
SELECT id as keeper_id, ip
FROM ranked
WHERE rn = 1;

-- Step 2: Create mapping from duplicate device_id to keeper device_id
CREATE TEMP TABLE device_redirect AS
SELECT
    d.id as old_id,
    dk.keeper_id as new_id
FROM devices d
JOIN device_keepers dk ON d.ip = dk.ip
WHERE d.id != dk.keeper_id
  AND d.ip IS NOT NULL
  AND d.ip != '';

-- Step 3: Update interfaces.device_id to point to keeper
-- Handle collisions on (device_id, name) by keeping the most recent interface
UPDATE interfaces
SET device_id = (SELECT new_id FROM device_redirect WHERE old_id = interfaces.device_id)
WHERE device_id IN (SELECT old_id FROM device_redirect);

-- Step 3b: Resolve interface collisions after reassignment
-- For each (device_id, name) that now has duplicates, keep the one with latest updated_at
DELETE FROM interfaces
WHERE id IN (
    SELECT i1.id
    FROM interfaces i1
    JOIN interfaces i2
      ON i1.device_id = i2.device_id
     AND i1.name = i2.name
     AND i1.id != i2.id
     AND i1.updated_at < i2.updated_at
);

-- Step 4: Update services.device_id to point to keeper
-- Handle collisions on (device_id, port, protocol) by keeping the most recent service
UPDATE services
SET device_id = (SELECT new_id FROM device_redirect WHERE old_id = services.device_id)
WHERE device_id IN (SELECT old_id FROM device_redirect);

-- Step 4b: Resolve service collisions after reassignment
-- For each (device_id, port, protocol) that now has duplicates, keep the one with latest updated_at
DELETE FROM services
WHERE id IN (
    SELECT s1.id
    FROM services s1
    JOIN services s2
      ON s1.device_id = s2.device_id
     AND s1.port = s2.port
     AND s1.protocol = s2.protocol
     AND s1.id != s2.id
     AND s1.updated_at < s2.updated_at
);

-- Step 5: Update incidents.entity_id where entity_type = 'device'
UPDATE incidents
SET entity_id = (SELECT new_id FROM device_redirect WHERE old_id = incidents.entity_id)
WHERE entity_type = 'device'
  AND entity_id IN (SELECT old_id FROM device_redirect);

-- Step 6: Update topology_nodes where node_type = 'device'
UPDATE topology_nodes
SET id = (SELECT new_id FROM device_redirect WHERE old_id = topology_nodes.id)
WHERE node_type = 'device'
  AND id IN (SELECT old_id FROM device_redirect);

-- Step 7: Update topology_edges where source or target is a duplicate device
UPDATE topology_edges
SET source = (SELECT new_id FROM device_redirect WHERE old_id = topology_edges.source)
WHERE source IN (SELECT old_id FROM device_redirect);

UPDATE topology_edges
SET target = (SELECT new_id FROM device_redirect WHERE old_id = topology_edges.target)
WHERE target IN (SELECT old_id FROM device_redirect);

-- Step 8: Update observations.device_id
UPDATE observations
SET device_id = (SELECT new_id FROM device_redirect WHERE old_id = observations.device_id)
WHERE device_id IN (SELECT old_id FROM device_redirect);

-- Step 9: Update events.device_id (legacy events table)
UPDATE events
SET device_id = (SELECT new_id FROM device_redirect WHERE old_id = events.device_id)
WHERE device_id IN (SELECT old_id FROM device_redirect);

-- Step 10: Update check_results.entity_id where entity_type = 'Device'
UPDATE check_results
SET entity_id = (SELECT new_id FROM device_redirect WHERE old_id = check_results.entity_id)
WHERE entity_type = 'Device'
  AND entity_id IN (SELECT old_id FROM device_redirect);

-- Step 11: Update events_v2.entity_id where entity_type = 'Device'
UPDATE events_v2
SET entity_id = (SELECT new_id FROM device_redirect WHERE old_id = events_v2.entity_id)
WHERE entity_type = 'Device'
  AND entity_id IN (SELECT old_id FROM device_redirect);

-- Step 12: Delete the duplicate devices
DELETE FROM devices
WHERE id IN (SELECT old_id FROM device_redirect);

-- Cleanup temp tables (auto-dropped at end of transaction)
-- DROP TABLE device_redirect;
-- DROP TABLE device_keepers;

-- ========================================================================
-- PART 2: UNIQUE INDEXES AND SCHEMA ADDITIONS (after deduplication)
-- ========================================================================

-- 1. Recreate idx_devices_ip as UNIQUE (IP is stable key for v1; DHCP/multi-IP/IPv6 are v2 limitations)
DROP INDEX IF EXISTS idx_devices_ip;
CREATE UNIQUE INDEX idx_devices_ip ON devices(ip) WHERE ip IS NOT NULL;

-- 2. Unique constraint on interfaces(device_id, name)
CREATE UNIQUE INDEX idx_interfaces_device_name ON interfaces(device_id, name);

-- 3. Unique constraint on services(device_id, port, protocol)
CREATE UNIQUE INDEX idx_services_device_port_proto ON services(device_id, port, protocol);

-- 4. Add last_seen to interfaces (for absence detection)
ALTER TABLE interfaces ADD COLUMN last_seen TEXT;

-- 5. Add last_seen to devices (for absence detection)
ALTER TABLE devices ADD COLUMN last_seen TEXT;

-- 6. Create routes table (IPv4 only in v1; IPv6 in v2)
CREATE TABLE IF NOT EXISTS routes (
    id TEXT PRIMARY KEY,
    destination TEXT NOT NULL,
    gateway TEXT, -- '0.0.0.0' when no gateway (instead of NULL)
    interface TEXT NOT NULL,
    metric INTEGER,
    mask TEXT,
    family TEXT NOT NULL DEFAULT 'ipv4',
    last_seen TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 7. Unique index on routes: destination + interface + gateway + metric
CREATE UNIQUE INDEX idx_routes_dest_iface_gw_metric ON routes(destination, interface, gateway, metric);

-- 7b. Index for queries by interface
CREATE INDEX IF NOT EXISTS idx_routes_interface ON routes(interface);

-- 7c. Index for queries by destination
CREATE INDEX IF NOT EXISTS idx_routes_destination ON routes(destination);

-- 8. Index on interfaces.last_seen for absence pruning
CREATE INDEX IF NOT EXISTS idx_interfaces_last_seen ON interfaces(last_seen);

-- 9. Index on devices.last_seen for absence pruning
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen);

-- 10. Index on routes.last_seen for absence pruning
CREATE INDEX IF NOT EXISTS idx_routes_last_seen ON routes(last_seen);