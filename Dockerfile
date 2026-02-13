# Build stage
FROM registry.suse.com/bci/rust:1.92 AS builder

WORKDIR /usr/src/outrider

# Copy manifests
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src
COPY src src

# Build the application
RUN cargo build --release

# Runtime stage
FROM registry.suse.com/bci/bci-minimal:15.7

# Copy the binary from builder
COPY --from=builder /usr/src/outrider/target/release/outrider /usr/local/bin/outrider

USER 1001

ENTRYPOINT ["/usr/local/bin/outrider"]