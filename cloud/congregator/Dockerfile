FROM node:24-bookworm-slim AS web-build
WORKDIR /workspace
COPY package.json package-lock.json ./
COPY clients/web/package.json clients/web/package.json
COPY cloud/congregator/web/package.json cloud/congregator/web/package.json
RUN npm ci
COPY cloud/congregator/web cloud/congregator/web
RUN npm run build --workspace @solmu/congregator-web

FROM rust:1-bookworm AS rust-build
WORKDIR /workspace
COPY . .
RUN cargo build --locked --release -p solmu-congregator

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home congregator
WORKDIR /app
COPY --from=rust-build /workspace/target/release/congregator /usr/local/bin/congregator
COPY --from=web-build /workspace/cloud/congregator/web/dist /app/web
ENV CONGREGATOR_WEB_DIR=/app/web
EXPOSE 8080
USER 10001:10001
ENTRYPOINT ["congregator"]
