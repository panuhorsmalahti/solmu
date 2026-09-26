FROM rust:1.98.1-slim-bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY backend/Cargo.toml backend/Cargo.lock ./backend/
COPY backend/src ./backend/src
RUN cargo build --release --locked --manifest-path backend/Cargo.toml

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /build/backend/target/release/solmu-backend /usr/local/bin/solmu-backend
ENV SOLMU_BIND_ADDR=0.0.0.0:3000
EXPOSE 3000
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/solmu-backend"]
