# Build stage
FROM rust:1.83-slim as builder

WORKDIR /usr/src/outrider

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml ./

# Copy source
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /usr/src/outrider/target/release/outrider /usr/local/bin/outrider

# Create non-root user
RUN useradd -r -u 1000 -s /bin/false outrider

USER outrider

ENTRYPOINT ["/usr/local/bin/outrider"]