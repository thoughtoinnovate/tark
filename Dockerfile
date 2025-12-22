# Super minimal Docker image for tark server
# Automatically builds for host architecture (x86_64 or ARM64)
# Final image size: ~15-20MB (just binary + certs)

# =============================================================================
# Stage 1: Build the binary
# =============================================================================
FROM rust:1.85-alpine AS builder

# Detect architecture for conditional operations
ARG TARGETARCH
ARG TARGETPLATFORM

# Install build dependencies for static linking
# gcc/g++ are needed for tree-sitter C compilation
# UPX is only reliable on x86_64, skip on ARM
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    gcc \
    g++ \
    make \
    pkgconfig \
    && if [ "$TARGETARCH" = "amd64" ]; then apk add --no-cache upx; fi

WORKDIR /build

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock* ./

# Create dummy src to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

# Build dependencies only (cached layer)
RUN cargo build --release 2>/dev/null || true

# Copy actual source
COPY src ./src

# Build the actual binary with optimizations
# Strip debug symbols (~50% size reduction)
# Compress with UPX only on x86_64 (~60% additional reduction)
RUN cargo build --release --bin tark && \
    strip /build/target/release/tark && \
    if [ "$TARGETARCH" = "amd64" ] && command -v upx >/dev/null 2>&1; then \
        upx --best --lzma /build/target/release/tark || true; \
    fi

# =============================================================================
# Stage 2: Minimal runtime (scratch-based)
# =============================================================================
FROM scratch

# Labels for container registry
LABEL org.opencontainers.image.source="https://github.com/thoughtoinnovate/tark"
LABEL org.opencontainers.image.description="tark AI coding assistant - minimal image"
LABEL org.opencontainers.image.licenses="MIT"

# Copy CA certificates for HTTPS (needed for API calls)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the binary
COPY --from=builder /build/target/release/tark /tark

# Expose HTTP server port
EXPOSE 8765

# Run as non-root (UID 1000)
USER 1000

# No shell available - direct binary execution
ENTRYPOINT ["/tark"]
CMD ["serve", "--host", "0.0.0.0", "--port", "8765"]
