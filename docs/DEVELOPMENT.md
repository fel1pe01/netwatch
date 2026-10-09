# Development Guide

## Project structure

```
netwatch/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs
│   ├── logging.rs
│   ├── storage.rs
│   ├── api.rs
│   ├── metrics.rs
│   ├── scheduler.rs
│   ├── events.rs
│   └── ... (future modules)
├── migrations/
│   └── 20231006000000_initial.sql
└── tests/
    └── integration_test.rs
```

All core functionality is split into small, single‑purpose modules.

## Running unit tests

```bash
cargo test
```

The tests include:
- Configuration parsing.
- SQLite migration and basic queries.
- Scheduler behaviour (job scheduling, timeout, concurrency).

## Building for release

```bash
cargo build --release
```

The binary will be available at `target/release/netwatch`.
