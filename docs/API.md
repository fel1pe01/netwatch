# Netwatch HTTP API Reference

This document describes the HTTP API endpoints exposed by Netwatch.

## Base URL

```
http://<host>:<port>
```

Default: `http://127.0.0.1:8080` (secure by default - binds to loopback only)

## Authentication

Token-based authentication is enabled by default (Bearer token). The `/health` and `/metrics` endpoints are public and do not require authentication.

**Roles:**
- **Administrator**: Full access (read, write, configure, admin)
- **Operator**: Read and write access
- **Viewer**: Read-only access; MAC addresses are omitted from responses

**Request header:** `Authorization: Bearer <token>`

**Error responses:**
- `401 Unauthorized`: Missing or invalid token
- `403 Forbidden`: Insufficient role for the requested operation

## Endpoints

### Health Check

#### GET /health

Health check endpoint for liveness/readiness probes. **Public endpoint - no authentication required.**

**Response:**
```json
{
  "status": "ok"
}
```

**Status Codes:**
- `200 OK` - Service is healthy

---

### Metrics

#### GET /metrics

Prometheus metrics endpoint. Returns metrics in Prometheus text format. **Public endpoint - no authentication required.**

**Response:** Plain text with Prometheus metrics

**Example metrics:**
```
# HELP netwatch_info netwatch instance info
# TYPE netwatch_info gauge
netwatch_info 1

# HELP netwatch_uptime_seconds uptime in seconds
# TYPE netwatch_uptime_seconds gauge
netwatch_uptime_seconds 12345

# HELP netwatch_health_checks_total Total number of health checks performed
# TYPE netwatch_health_checks_total counter
netwatch_health_checks_total{protocol="icmp"} 42

# HELP netwatch_health_check_failures_total Total number of health check failures
# TYPE netwatch_health_check_failures_total counter
netwatch_health_check_failures_total{protocol="icmp"} 3

# HELP netwatch_health_check_latency_seconds Latency of health checks in seconds
# TYPE netwatch_health_check_latency_seconds histogram
netwatch_health_check_latency_seconds_bucket{protocol="icmp",le="0.05"} 10
```

---

### Network Discovery

#### GET /api/v1/interfaces

List persisted network interfaces. Requires authentication (Viewer role minimum).

**Response:**
```json
{
  "items": [
    {
      "id": "eth0",
      "device_id": "172.21.86.78",
      "name": "eth0",
      "mtu": 1492,
      "state": "up",
      "source": "discovery",
      "confidence": "high",
      "observed_at": "up",
      "created_at": "2026-11-22T20:21:57.163262192+00:00",
      "updated_at": "2026-11-22T20:21:57.163262192+00:00",
      "last_seen": "2026-11-22T20:21:57.163262192+00:00",
      "present": true,
      "mac": "aa:bb:cc:dd:ee:ff"
    }
  ],
  "total": 1,
  "limit": 50,
  "offset": 0
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Interface identifier (name) |
| `device_id` | string | Parent device ID |
| `name` | string | Interface name |
| `mtu` | integer | Maximum transmission unit |
| `state` | string | Operational state (`up`, `down`, `unknown`) |
| `source` | string | Discovery source |
| `confidence` | string | Confidence level |
| `observed_at` | string | Observation timestamp |
| `created_at` | string | Creation timestamp (RFC3339) |
| `updated_at` | string | Last update timestamp (RFC3339) |
| `last_seen` | string | Last seen timestamp (RFC3339) |
| `present` | boolean | Whether interface is currently present |
| `mac` | string | MAC address (Admin/Operator only; omitted for Viewer) |

**Query Parameters:**
- `limit` (1-500, default 50) — Page size
- `offset` (≥0, default 0) — Page offset
- `device_id` — Filter by device ID
- `name` — Filter by interface name
- `present` (`true`|`false`) — Filter by presence status
- `order_by` — Sort field (`id`, `device_id`, `name`, `mac`, `mtu`, `state`, `created_at`, `updated_at`)
- `order_dir` — Sort direction (`asc`|`desc`, default `desc`)

**MAC Address Visibility:** The `mac` field is only returned for Admin and Operator roles. Viewer role receives the field as omitted (not null).

---

#### GET /api/v1/routes

List persisted routing table entries. Requires authentication (Viewer role minimum).

**Response:**
```json
{
  "items": [
    {
      "id": "0.0.0.0|eth0",
      "destination": "0.0.0.0",
      "gateway": "172.21.80.1",
      "interface": "eth0",
      "metric": 0,
      "mask": "",
      "family": "ipv4",
      "last_seen": "2026-11-22T20:21:57.163262192+00:00",
      "created_at": "2026-11-22T20:21:57.163262192+00:00",
      "updated_at": "2026-11-22T20:21:57.163262192+00:00",
      "present": true
    }
  ],
  "total": 2,
  "limit": 50,
  "offset": 0
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Route identifier (destination|interface) |
| `destination` | string | Destination CIDR |
| `gateway` | string | Gateway IP or `null` (direct) |
| `interface` | string | Interface name |
| `metric` | integer | Route metric |
| `mask` | string | Netmask |
| `family` | string | Address family (`ipv4`) |
| `last_seen` | string | Last seen timestamp (RFC3339) |
| `created_at` | string | Creation timestamp (RFC3339) |
| `updated_at` | string | Last update timestamp (RFC3339) |
| `present` | boolean | Whether route is currently present |

**Query Parameters:**
- `limit` (1-500, default 50) — Page size
- `offset` (≥0, default 0) — Page offset
- `destination` — Filter by destination
- `interface` — Filter by interface
- `gateway` — Filter by gateway
- `present` (`true`|`false`) — Filter by presence status
- `order_by` — Sort field (`id`, `destination`, `gateway`, `interface`, `metric`, `family`, `created_at`, `updated_at`)
- `order_dir` — Sort direction (`asc`|`desc`, default `desc`)

---

### Device Inventory

#### GET /api/v1/devices

List persisted devices with pagination. Requires authentication (Viewer role minimum).

**Response:**
```json
{
  "items": [
    {
      "id": "device-1",
      "hostname": "router-1",
      "ip": "192.168.1.1",
      "mac": "aa:bb:cc:dd:ee:ff",
      "device_type": "router",
      "source": "discovery",
      "confidence": "high",
      "observed_at": "2024-01-15T10:30:00Z",
      "created_at": "2024-01-15T10:30:00Z",
      "updated_at": "2024-01-15T10:30:00Z",
      "present": true
    }
  ],
  "total": 1,
  "limit": 50,
  "offset": 0
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Device identifier |
| `hostname` | string | Device hostname |
| `ip` | string | IP address |
| `mac` | string | MAC address (Admin/Operator only) |
| `device_type` | string | Device type (router, switch, host, etc.) |
| `source` | string | Discovery source |
| `confidence` | string | Confidence level (high/medium/low) |
| `observed_at` | string | Observation timestamp (RFC3339) |
| `created_at` | string | Creation timestamp (RFC3339) |
| `updated_at` | string | Last update timestamp (RFC3339) |
| `present` | boolean | Always `true` (devices don't have last_seen) |

**Query Parameters:**
- `limit` (1-500, default 50) — Page size
- `offset` (≥0, default 0) — Page offset
- `id` — Filter by device ID
- `ip` — Filter by IP address (exact match)
- `hostname` — Filter by hostname (exact match)
- `device_type` — Filter by device type
- `order_by` — Sort field (`id`, `hostname`, `ip`, `device_type`, `created_at`, `updated_at`)
- `order_dir` — Sort direction (`asc`|`desc`, default `desc`)

**Note:** The `present` filter is **not available** on `/devices` (devices don't have `last_seen`, so they are always present).

**MAC Address Visibility:** The `mac` field is only returned for Admin and Operator roles. Viewer role receives the field as omitted (not null).

---

#### GET /api/v1/hosts

Alias for `GET /api/v1/devices` (maintained for compatibility).
- **Response format:** Same as `/api/v1/devices`
- **Source:** SQLite `devices` table (real data)
- **Query parameters:** Same as `/api/v1/devices`
- **Note:** The `present` filter is **not available** on `/hosts` (same as `/devices`)

---

### Monitoring Endpoints

#### POST /api/v1/monitor/ping

Perform an ICMP ping check. Requires authentication (Operator role minimum).

**Request:**
```json
{
  "target": "192.168.1.1",
  "timeout_ms": 2000
}
```

**Response:**
```json
{
  "target": "192.168.1.1",
  "protocol": "icmp",
  "timestamp": "2024-01-15T10:30:00Z",
  "success": true,
  "latency_ms": 1.23,
  "error": null,
  "observed_at": "2024-01-15T10:30:00Z",
  "timeout_ms": 2000,
  "evidence": "icmp"
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `target` | string | Target IP/hostname |
| `protocol` | string | Always `"icmp"` |
| `timestamp` | string | Check timestamp (RFC3339) |
| `success` | boolean | Check result |
| `latency_ms` | number | Round-trip time in milliseconds |
| `error` | string/null | Error message if failed |
| `observed_at` | string | Observation timestamp |
| `timeout_ms` | integer | Configured timeout |
| `evidence` | string | Always `"icmp"` |

**Status Codes:**
- `200 OK` - Check completed (success or failure)
- `400 Bad Request` - Invalid request body
- `401 Unauthorized` - Missing or invalid token
- `403 Forbidden` - Insufficient role (requires Operator)
- `415 Unsupported Media Type` - Invalid Content-Type
- `503 Service Unavailable` - Internal error (DB unavailable)

---

#### POST /api/v1/monitor/tcp

Perform a TCP connection check. Requires authentication (Operator role minimum).

**Request:**
```json
{
  "host": "192.168.1.1",
  "port": 80,
  "timeout_ms": 2000
}
```

**Response:**
```json
{
  "target": "192.168.1.1:80",
  "protocol": "tcp",
  "timestamp": "2024-01-15T10:30:00Z",
  "success": true,
  "latency_ms": 2.45,
  "error": null,
  "observed_at": "2024-01-15T10:30:00Z",
  "timeout_ms": 2000,
  "evidence": "tcp_probe"
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `target` | string | Target host:port |
| `protocol` | string | Always `"tcp"` |
| `timestamp` | string | Check timestamp |
| `success` | boolean | Connection result |
| `latency_ms` | number | Connection time in milliseconds |
| `error` | string/null | Error message if failed |
| `observed_at` | string | Observation timestamp |
| `timeout_ms` | integer | Configured timeout |
| `evidence` | string | Always `"tcp_probe"` |

**Status Codes:**
- `200 OK` - Check completed
- `400 Bad Request` - Invalid request body
- `401 Unauthorized` - Missing or invalid token
- `403 Forbidden` - Insufficient role (requires Operator)
- `415 Unsupported Media Type` - Invalid Content-Type
- `503 Service Unavailable` - Internal error (DB unavailable)

---

### Checks & Events

#### GET /api/v1/checks

List check results with pagination. Requires authentication (Viewer role minimum).

**Response:**
```json
{
  "items": [
    {
      "id": "check-1",
      "entity_id": "device-1",
      "entity_type": "device",
      "protocol": "icmp",
      "target": "192.168.1.1",
      "success": true,
      "latency_ms": 1.23,
      "error": null,
      "observed_at": "2024-01-15T10:30:00Z",
      "created_at": "2024-01-15T10:30:00Z"
    }
  ],
  "total": 1,
  "limit": 50,
  "offset": 0
}
```

**Query Parameters:**
- `limit` (1-500, default 50) — Page size
- `offset` (≥0, default 0) — Page offset
- `entity_id` — Filter by entity ID
- `entity_type` — Filter by entity type (device, interface, service)
- `protocol` — Filter by protocol (icmp, tcp, dns, http, https, tls)
- `success` (`true`|`false`) — Filter by result
- `since` — Filter by observed_at (RFC3339)
- `until` — Filter by observed_at (RFC3339)

---

#### GET /api/v1/events

List events with pagination. Requires authentication (Viewer role minimum).

**Response:**
```json
{
  "items": [
    {
      "id": "evt-1",
      "schema_version": 1,
      "event_type": "CheckCompleted",
      "event_ts": "2024-01-15T10:30:00Z",
      "observed_ts": "2024-01-15T10:30:00Z",
      "entity_id": "device-1",
      "entity_type": "device",
      "severity": "info",
      "state": "up",
      "source": "scheduler",
      "evidence": "icmp",
      "dedup_key": "device-1:icmp",
      "correlation_id": "corr-1",
      "trace_id": "trace-1",
      "metadata": {}
    }
  ],
  "total": 1,
  "limit": 50,
  "offset": 0
}
```

**Query Parameters:**
- `limit` (1-500, default 50) — Page size
- `offset` (≥0, default 0) — Page offset
- `event_type` — Filter by event type
- `entity_id` — Filter by entity ID
- `entity_type` — Filter by entity type
- `severity` — Filter by severity (info, warning, critical)
- `since` — Filter by event_ts (RFC3339)
- `until` — Filter by event_ts (RFC3339)

---

### Incidents

#### GET /api/v1/incidents

List incidents with pagination. Requires authentication (Viewer role minimum).

**Response:**
```json
{
  "items": [
    {
      "id": "inc-1",
      "dedup_key": "device-1:icmp",
      "entity_id": "device-1",
      "entity_type": "device",
      "state": "open",
      "severity": "critical",
      "title": "ICMP Check Down",
      "description": "ICMP check failing for device-1",
      "labels": { "team": "network" },
      "evidence": [],
      "opened_at": "2024-01-15T10:30:00Z",
      "acknowledged_at": null,
      "resolved_at": null,
      "closed_at": null,
      "acknowledged_by": null,
      "cooldown_until": "2024-01-15T10:35:00Z",
      "silence_until": null,
      "root_cause_hypothesis_id": null,
      "root_cause_confirmed": false,
      "root_cause_confirmed_at": null,
      "root_cause_confirmed_by": null,
      "correlation_id": "corr-1",
      "transitions": []
    }
  ],
  "total": 1,
  "limit": 50,
  "offset": 0
}
```

#### POST /api/v1/incidents/{incident_id}/acknowledge

Acknowledge an incident. Requires authentication (Operator role minimum).

**Request:**
```json
{
  "actor": "admin",
  "reason": "Investigating"
}
```

**Response:**
```json
{
  "status": "ok",
  "message": "incident acknowledged"
}
```

#### POST /api/v1/incidents/{incident_id}/resolve

Resolve an incident. Requires authentication (Operator role minimum).

**Request:**
```json
{
  "actor": "admin",
  "reason": "Issue fixed"
}
```

#### POST /api/v1/incidents/{incident_id}/close

Close an incident. Requires authentication (Administrator role minimum).

**Request:**
```json
{
  "actor": "admin"
}
```

---

### Root Cause Analysis

#### GET /api/v1/incidents/hypotheses

List root cause hypotheses for open incidents. Requires authentication (Viewer role minimum).

**Response:**
```json
{
  "hypotheses": [
    {
      "id": "hyp-1",
      "entity_id": "device-1",
      "entity_type": "device",
      "entity_label": "router-1",
      "confidence": 0.85,
      "evidence": [],
      "dependency_path": [],
      "created_at": "2024-01-15T10:30:00Z",
      "confirmed": false
    }
  ]
}
```

#### GET /api/v1/incidents/hypotheses/{hypothesis_id}

Get detailed hypothesis information. Requires authentication (Viewer role minimum).

#### POST /api/v1/incidents/{incident_id}/hypothesis/confirm

Confirm or reject a root cause hypothesis. Requires authentication (Operator role minimum).

**Request:**
```json
{
  "hypothesis_id": "hyp-1",
  "confirmed": true,
  "actor": "admin"
}
```

**Response:**
```json
{
  "status": "ok",
  "message": "hypothesis confirmation recorded"
}
```

---

### Placeholder Endpoints

These endpoints are placeholders (not yet implemented, return empty paginated responses):

| Endpoint | Description | Status |
|----------|-------------|--------|
| `GET /api/v1/vlans` | VLAN inventory | Placeholder |
| `GET /api/v1/subnets` | Subnet inventory | Placeholder |
| `GET /api/v1/gateways` | Gateway inventory | Placeholder |
| `GET /api/v1/topology` | Network topology | Placeholder |

**Response for placeholders:**
```json
{
  "items": [],
  "total": 0,
  "limit": 50,
  "offset": 0
}
```

---

### Capacity Planning

#### POST /api/v1/capacity/report

Generate a capacity planning report. Requires authentication (Operator role minimum).

**Request:**
```json
{
  "format": "markdown"
}
```

**Response (markdown):**
```markdown
# Capacity Planning Report

## Summary
- Generated: 2024-01-15T10:30:00Z
- Analysis window: 30 days

## Resources
| Resource | Current Usage | Projected Saturation |
|----------|---------------|----------------------|
| cpu | 45% | 2024-06-15 |
| memory | 62% | 2024-04-20 |
| disk | 38% | 2025-01-10 |

## Recommendations
- Monitor memory growth rate
- Plan disk expansion by Q4 2024
```

**Request Fields:**
| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `format` | string | `"markdown"` | Output format: `markdown` or `json` |

---

### Alertmanager Webhook

#### POST /api/v1/alerts/webhook

Receive alerts from Alertmanager. **Public endpoint - no authentication required.**

**Request:** Alertmanager webhook payload

**Response:**
```json
{
  "status": "ok"
}
```

---

### Error Responses

All endpoints may return standard error responses:

**400 Bad Request**
```json
{
  "error": "Invalid request: missing required field 'target'"
}
```

**401 Unauthorized**
```json
{
  "error": "Unauthorized"
}
```

**403 Forbidden**
```json
{
  "error": "Forbidden: insufficient role"
}
```

**415 Unsupported Media Type**
```json
{
  "error": "Content-Type must be application/json"
}
```

**503 Service Unavailable**
```json
{
  "error": "Internal server error"
}
```

All error responses use uniform JSON format: `{"error": "..."}`

---

## Rate Limiting

IP-based rate limiting is enabled by default (100 requests per 60 seconds). The `/health` and `/metrics` endpoints are excluded from rate limiting.

**Response headers:**
- `X-RateLimit-Limit`: Maximum requests per window
- `X-RateLimit-Remaining`: Remaining requests in current window
- `X-RateLimit-Reset`: Unix timestamp when the window resets

---

## CORS

No CORS headers are currently set. All origins are allowed by default.

---

## Versioning

No API versioning is currently implemented. Breaking changes may occur.

---

## See Also

- [CONFIGURATION.md](CONFIGURATION.md) - Configuration options
- [OPERATIONS.md](OPERATIONS.md) - Operational procedures
- [README.md](../README.md) - Quick start guide