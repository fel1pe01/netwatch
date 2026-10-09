# Netwatch

Netwatch is a modular observability agent designed to run on Linux/WSl/and server environments. It periodically discovers hosts, monitors health, polls SNMP, tracks events, and exposes a REST API + Prometheus metrics for monitoring.

Netwatch is a modular network monitoring agent written in Rust. It provides a command-line interface, an HTTP API, network discovery, ICMP/TCP health checks, and Prometheus metrics.

## Current state

- Core project scaffolded with Rust + Tokio + Axum.
- Basic configuration parsing (`config.yaml`).
- SQLite persistence with migrations for inventory entities.
- Scheduler skeleton that can enqueue jobs.
- HTTP API with `/health` and `/metrics` endpoints (Prometheus scrape). Rest endpoints return empty collections.
- CLI with several subcommands (most are placeholders).

## Features

- CLI commands for ping, TCP checks, local discovery, help, and version.
- HTTP endpoints for health, network interfaces, routes, and monitoring.
- Prometheus metrics for application uptime and health checks.
- Health-check metrics with bounded cardinality using a `protocol` label.
- Configuration parsing and automated tests.
- End-to-end smoke-test script.

## Requirements

- Rust toolchain and Cargo.
- Linux or a compatible WSL environment.
- The `ping` executable for ICMP checks.

## Building

```bash
# Clone the repo
git clone <repo-url>
cd netwatch

# Build
cargo build --release
```

## Running

```bash
# Run agent with default config
# The default server address is 0.0.0.0:8080.
./target/release/netwatch
```

```bash
# See the CLI help for available commands:
./target/release/netwatch --help
```

The agent reads configuration from `$HOME/.config/netwatch/netwatch.yaml`. You can override with `--config <path>`.

## Testing

Run the automated test suite:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```
Run the end-to-end smoke tests:

```bash
./scripts/e2e_smoke.sh
```

## Documentation

- `docs/ARCHITECTURE_AND_FUNCTIONAL_TESTS.md` — architecture and functional audit.
- `docs/CHANGES_MADE.md` — implementation change log.
- `docs/DEVELOPMENT.md` — development notes.

## Current limitations

Some components, including scheduling, event handling, and persistence, are not fully wired into the running server. Network discovery currently depends on Linux interfaces. ICMP checks require the external ping executable. Review the architecture audit for the detailed implementation status and outstanding work.
