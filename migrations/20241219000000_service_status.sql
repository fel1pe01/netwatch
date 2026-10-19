-- migration to add service status tracking
-- Version format: YYYYMMDDHHMMSS_service_status

-- Add service status tracking columns to services table
-- Note: status column already exists from initial migration
ALTER TABLE services ADD COLUMN last_check_at TEXT;
ALTER TABLE services ADD COLUMN last_check_success BOOLEAN;
ALTER TABLE services ADD COLUMN last_check_latency_ms INTEGER;
ALTER TABLE services ADD COLUMN check_interval INTEGER DEFAULT 60;

-- Index for service status queries
CREATE INDEX IF NOT EXISTS idx_services_last_check ON services(last_check_at);
CREATE INDEX IF NOT EXISTS idx_services_device_status ON services(device_id, status);