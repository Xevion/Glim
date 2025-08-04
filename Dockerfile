# Build Stage
FROM rust:1.82.0-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src
RUN USER=root cargo new --bin livecards
WORKDIR /usr/src/livecards

# Copy dependency files for better layer caching
COPY ./Cargo.toml ./Cargo.lock* ./build.rs ./
COPY ./card.svg ./

# Build empty app with downloaded dependencies to produce a stable image layer for next build
# Note: Docker image builds server-only version (no CLI dependencies)
RUN cargo build --release --no-default-features --features server

# Build web app with own code
RUN rm src/*.rs
COPY ./src ./src
RUN rm ./target/release/deps/livecards*
RUN cargo build --release --no-default-features --features server

# Strip the binary to reduce size
RUN strip target/release/livecards

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
    && rm -rf /var/lib/apt/lists/*

ENV TZ=Etc/UTC

# Create user with specific UID/GID
RUN addgroup --gid $GID $APP_USER \
    && adduser --uid $UID --disabled-password --gecos "" --ingroup $APP_USER $APP_USER \
    && mkdir -p ${APP}

# Copy application files
COPY --from=builder --chown=$APP_USER:$APP_USER /usr/src/livecards/target/release/livecards ${APP}/livecards
COPY --from=builder --chown=$APP_USER:$APP_USER /usr/src/livecards/src/fonts ${APP}/fonts

# Set proper permissions
RUN chmod +x ${APP}/livecards

USER $APP_USER
WORKDIR ${APP}

# Use ARG for build-time configuration, ENV for runtime
ARG PORT=8000
ENV PORT=${PORT}
EXPOSE ${PORT}

# Add health check (using wget since curl isn't in Alpine by default)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl --fail http://localhost:${PORT}/health || exit 1

# Use ENTRYPOINT for the executable and CMD for default arguments
ENTRYPOINT ["/usr/src/app/livecards"]
CMD ["0.0.0.0:${PORT}"]