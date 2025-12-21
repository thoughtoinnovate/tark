# Multi-stage build for tark server
# Stage 1: Build
FROM rust:1.75-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static

WORKDIR /build

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock* ./

# Create dummy src to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true

# Copy actual source
COPY src ./src

# Build the actual binary
RUN cargo build --release --bin tark

# Stage 2: Runtime
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    curl \
    git

# Create non-root user
RUN adduser -D -s /bin/sh tark

# Copy binary from builder
COPY --from=builder /build/target/release/tark /usr/local/bin/tark

# Make binary executable
RUN chmod +x /usr/local/bin/tark

# Switch to non-root user
USER tark
WORKDIR /home/tark

# Expose HTTP server port
EXPOSE 8765

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8765/health || exit 1

# Default command: run the server
ENTRYPOINT ["tark"]
CMD ["serve", "--host", "0.0.0.0", "--port", "8765"]
