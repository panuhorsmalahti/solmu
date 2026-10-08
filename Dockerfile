FROM node:24-bookworm-slim AS web-build
WORKDIR /web
COPY package.json package-lock.json ./
COPY clients/web ./clients/web
COPY cloud/congregator/web/package.json ./cloud/congregator/web/package.json
RUN npm ci && npm run build:web

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
COPY boxer ./boxer
COPY boxer-gui ./boxer-gui
COPY muxer ./muxer
COPY muxer-gui ./muxer-gui
COPY cloud/congregator/backend ./cloud/congregator/backend
RUN cargo build --release --locked -p solmu-backend

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates bash \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN mkdir /data && chown 10001:10001 /data
COPY --from=build /build/target/release/solmu-backend /usr/local/bin/solmu-backend
COPY --from=web-build /web/clients/web/dist /app/web
ENV SOLMU_BIND_ADDR=0.0.0.0:3000
ENV SOLMU_WEBHOOK_BIND_ADDR=0.0.0.0:3001
ENV SOLMU_DATABASE_URL=sqlite:///data/solmu.db
ENV SOLMU_WORKSPACE=/data/workspace
ENV SOLMU_WEB_DIR=/app/web
EXPOSE 3000 3001
VOLUME ["/data"]
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/solmu-backend"]
