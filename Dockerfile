# Note: This Dockerfile is used for this project's Railway deployment and image testing. It may not be suitable for your use case.

# Build Stage
ARG RUST_VERSION=1.86.0
FROM rust:${RUST_VERSION}-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src
RUN USER=root cargo new --bin glim
WORKDIR /usr/src/glim

# Copy dependency files for better layer caching
COPY ./Cargo.toml ./Cargo.lock* ./build.rs ./card.svg ./

# Build empty app with downloaded dependencies to produce a stable image layer for next build
# Note: Docker image builds server-only version (no CLI dependencies)
RUN cargo build --release --no-default-features --features server

# Build web app with own code
RUN rm src/*.rs
COPY ./src ./src
RUN rm ./target/release/deps/glim*
RUN cargo build --release --no-default-features --features server

# Strip the binary to reduce size
RUN strip target/release/glim

# Runtime Stage - Debian slim for glibc compatibility
FROM debian:12-slim

ARG APP=/usr/src/app
ARG APP_USER=appuser
ARG UID=1000
ARG GID=1000

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tzdata \
    wget \
    && rm -rf /var/lib/apt/lists/*

ARG TZ=Etc/UTC
ENV TZ=${TZ}

# Create user with specific UID/GID
RUN addgroup --gid $GID $APP_USER \
    && adduser --uid $UID --disabled-password --gecos "" --ingroup $APP_USER $APP_USER \
    && mkdir -p ${APP}

# Copy application files
COPY --from=builder --chown=$APP_USER:$APP_USER /usr/src/glim/target/release/glim ${APP}/glim
COPY --from=builder --chown=$APP_USER:$APP_USER /usr/src/glim/src/fonts ${APP}/fonts

# Set proper permissions
RUN chmod +x ${APP}/glim

USER $APP_USER
WORKDIR ${APP}

# Build-time arg for PORT, default to 8000
ARG PORT=8000
# Runtime environment var for PORT, default to build-time arg
ENV PORT=${PORT}
EXPOSE ${PORT}

# Add health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:${PORT}/health || exit 1

CMD ["sh", "-c", "exec ./glim 0.0.0.0:${PORT},[::]:${PORT}"]
