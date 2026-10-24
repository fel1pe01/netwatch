# Netwatch HTTP API Reference

This document describes the HTTP API endpoints exposed by Netwatch.

## Base URL

```
http://<host>:<port>
```

Default: `http://localhost:8080`

## Authentication

Currently no authentication is implemented. All endpoints are publicly accessible.

## Endpoints

### Health Check

#### GET /health

Health check endpoint for liveness/readiness probes.

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

Prometheus metrics endpoint. Returns metrics in Prometheus text format.

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

List discovered network interfaces.

**Response:**
```json
{
  "interfaces": [
    {
      "name": "eth0",
      "oper_state": "up",
      "ipv4": "192.168.1.100",
      "ipv6": "fe80::1234:5678:9abc:def0",
      "mac": "aa:bb:cc:dd:ee:ff",
      "mtu": 1500,
      "evidence": "local_interface"
    }
  ]
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Interface name |
| `oper_state` | string | Operational state (`up`, `down`, `unknown`) |
| `ipv4` | string | IPv4 address or `null` |
| `ipv6` | string | IPv6 address or `null` |
| `mac` | string | MAC address |
| `mtu` | integer | Maximum transmission unit |
| `evidence` | string | Discovery source |

---

#### GET /api/v1/routes

List discovered routing table entries.

**Response:**
```json
{
  "routes": [
    {
      "destination": "0.0.0.0/0",
      "gateway": "192.168.1.1",
      "interface": "eth0",
      "metric": 100,
      "mask": "0.0.0.0",
      "evidence": "routing_table"
    }
  ]
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `destination` | string | Destination CIDR |
| `gateway` | string | Gateway IP or `null` |
| `interface` | string | Interface name |
| `metric` | integer | Route metric |
| `mask` | string | Netmask |
| `evidence` | string | Discovery source |

---

### Device Inventory

#### GET /api/v1/devices

List discovered devices.

**Response:**
```json
{
  "devices": [
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
      "updated_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

---

### Monitoring Endpoints

#### POST /api/v1/monitor/ping

Perform an ICMP ping check.

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
- `415 Unsupported Media Type` - Invalid Content-Type

---

#### POST /api/v1/monitor/tcp

Perform a TCP connection check.

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
- `415 Unsupported Media Type` - Invalid Content-Type

---

### Placeholder Endpoints

These endpoints return empty arrays (not yet implemented):

| Endpoint | Description |
|--------|-------------|
| `GET /api/v1/vlans` | VLAN inventory |
| `GET /api/v1/subnets` | Subnet inventory |
| `GET /api/v1/gateways` | Gateway inventory |
| `GET /api/v1/events` | Event log |
| `GET /api/v1/topology` | Network topology |

**Response:**
```json
[]
```

---

### Topology

#### GET /api/v1/topology

Returns current network topology (placeholder - returns empty array).

**Response:**
```json
[]
```

---

### Capacity Planning

#### POST /api/v1/capacity/report

Generate a capacity planning report.

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

Receive alerts from Alertmanager.

**Request:** Alertmanager webhook payload

**Response:**
```json
{
  "status": "ok"
}
```

---

### Root Cause Analysis

#### GET /api/v1/incidents/hypotheses

List root cause hypotheses for open incidents.

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

---

#### GET /api/v1/incidents/hypotheses/{hypothesis_id}

Get detailed hypothesis information.

**Response:**
```json
{
  "id": "hyp-1",
  "entity_id": "device-1",
  "entity_type": "device",
  "entity_label": "router-1",
  "confidence": 0.85,
  "evidence": [...],
  "dependency_path": [...],
  "created_at": "2024-01-15T10:30:00Z",
  "confirmed": false
}
```

---

#### POST /api/v1/incidents/{incident_id}/hypothesis/confirm

Confirm or reject a root cause hypothesis.

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

## Error Responses

All endpoints may return standard error responses:

**400 Bad Request**
```json
{
  "error": "Invalid request: missing required field 'target'"
}
```

**415 Unsupported Media Type**
```json
{
  "error": "Content-Type must be application/json"
}
```

**500 Internal Server Error**
```json
{
  "error": "Internal server error"
}
```

## Rate Limiting

No rate limiting is currently implemented.

## CORS

No CORS headers are currently set. All origins are allowed by default.

## Versioning

No API versioning is currently implemented. Breaking changes may occur.

## See Also

- [CONFIGURATION.md](CONFIGURATION.md) - Configuration options
- [OPERATIONS.md](OPERATIONS.md) - Operational procedures
- [README.md](../README.md) - Quick start guide