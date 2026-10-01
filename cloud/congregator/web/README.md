# Congregator web client

The React dashboard is built into the Congregator Rust service image. For local
UI development, run `npm run dev --workspace @solmu/congregator-web`; Vite proxies
API requests to `http://127.0.0.1:8080`.

![Congregator web dashboard](../../../docs/screenshots/congregator.png)

The dashboard keeps sandbox state up to date automatically.
