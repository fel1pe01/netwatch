# Build stage
FROM rust:1.79-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy source code
COPY src ./src
COPY migrations ./migrations

# Build application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libsqlite3-0 \
    iputils-ping \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false -u 1000 -g 1000 netwatch

# Create data directory
RUN mkdir -p /data && chown netwatch:netwatch /data

# Copy binary from builder
COPY --from=builder /app/target/release/netwatch /usr/local/bin/netwatch

# Switch to non-root user
USER netwatch

# Set working directory
WORKDIR /data

# Expose default port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/health || exit 1

# Default command
ENTRYPOINT ["netwatch"]
CMD ["--config", "/data/netwatch.yaml"]