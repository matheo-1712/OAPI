FROM rust:1.99-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY public ./public
RUN cargo build --locked --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 oapi \
    && useradd --uid 10001 --gid oapi --no-create-home oapi
WORKDIR /app
COPY --from=builder /build/target/release/OAPI /usr/local/bin/OAPI
COPY public ./public
COPY default_config.yaml badges.yaml ./
COPY deploy/config.production.yaml ./config.yaml
RUN mkdir generated_images && chown oapi:oapi generated_images
USER 10001:10001
ARG PB_URL=http://pocketbase:8090
ENV OAPI_ENV=production RUST_LOG=info PB_URL=${PB_URL}
EXPOSE 9003
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl --fail --silent http://127.0.0.1:9003/healthz || exit 1
CMD ["OAPI"]
