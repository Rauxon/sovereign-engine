# Stage 1: Build React UI
FROM node:22-alpine AS ui-builder
WORKDIR /build/ui
COPY ui/package.json ui/package-lock.json* ./
RUN npm ci
COPY ui/ .
RUN npm run build

# Stage 2: Build Rust proxy
FROM rust:1.92-bookworm AS rust-builder
WORKDIR /build/proxy
# Copy full source — sqlx::migrate!() needs the migrations directory at compile time
COPY proxy/ .
RUN cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the Rust binary
COPY --from=rust-builder /build/proxy/target/release/sovereign-engine /app/sovereign-engine

# Copy the React UI static files
COPY --from=ui-builder /build/ui/dist /app/ui

# Create non-root user. The Docker socket group (typically GID 999 or 998)
# is added at runtime via docker-compose group_add, not baked into the image.
RUN groupadd -r sovereign && useradd -r -g sovereign sovereign

# Create volume mount points owned by the non-root user
RUN mkdir -p /config /models && chown sovereign:sovereign /config /models

USER sovereign

# Default environment
ENV LISTEN_ADDR=0.0.0.0:443
ENV DATABASE_URL=sqlite:///config/sovereign.db
ENV MODEL_PATH=/models
ENV UI_PATH=/app/ui
ENV RUST_LOG=sovereign_engine=info,tower_http=info

EXPOSE 443 3000

ENTRYPOINT ["/app/sovereign-engine"]
