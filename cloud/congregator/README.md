# Congregator

Congregator is Solmu's Kubernetes control plane. Its Rust service manages
Solmu agent sandboxes through NVIDIA OpenShell, and its React dashboard shows
their live status and lifecycle controls.

See the [user and cluster installation guide](../../docs/congregator.md).

![Congregator dashboard](../../docs/screenshots/congregator.png)

## Development

Build the Rust service from the repository root:

```sh
cargo build -p solmu-congregator
```

Build and serve the web client:

```sh
npm run build --workspace @solmu/congregator-web
npm run dev --workspace @solmu/congregator-web
```

For local development, point `OPENSHELL_GATEWAY_URL` at a reachable OpenShell
gateway. Congregator serves its REST API and built web client from port `8080`.

## E2E

Congregator's browser end-to-end tests live in `e2e/congregator/`:

```sh
npx playwright test --config e2e/congregator/playwright.config.ts
```
