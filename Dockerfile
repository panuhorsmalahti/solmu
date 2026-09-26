FROM rust:1.98.1-slim-bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY backend ./backend
COPY clients/common ./clients/common
COPY clients/cli ./clients/cli
COPY clients/desktop ./clients/desktop
COPY e2e ./e2e
COPY sandbox ./sandbox
RUN cargo build --release --locked -p solmu-backend

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN mkdir /data && chown 10001:10001 /data
COPY --from=build /build/target/release/solmu-backend /usr/local/bin/solmu-backend
ENV SOLMU_BIND_ADDR=0.0.0.0:3000
ENV SOLMU_DATABASE_URL=sqlite:///data/solmu.db
EXPOSE 3000
VOLUME ["/data"]
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/solmu-backend"]
