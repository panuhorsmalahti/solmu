# Build and test Solmu

From the repository root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --all-targets --locked
npm ci
npm run lint:web
npm run build:web
npm run build:website
npx playwright install chromium
npm run test:e2e
```

Build the binaries before e2e tests. Tests launch a real Solmu backend with
temporary SQLite files and a local provider fixture; they never use your keys
or make paid model requests. Tests live in `e2e/api`, `e2e/cli`, `e2e/desktop`,
`e2e/web`, `e2e/ios`, `e2e/website`, `e2e/boxer`, `e2e/muxer`, and `e2e/muxer-gui`.
Release automation tests under `e2e/releases` use local Git repositories and
release metadata fixtures; they do not publish tags, images, or releases.
Within each client, keep feature tests in separate files such as `conversations`,
`responses`, `input`, and `workspaces`; share terminal helpers in `mod.rs`.
Muxer tests use a real outer terminal and real Solmu CLI processes in nested
pseudo-terminals, with the same local backend fixture.

The CLI tests use a real pseudo-terminal; on Linux they also run the client
inside the isolated sandbox. Linux tests must run in a delegated cgroup v2
hierarchy with CPU, memory, and task controllers:

```sh
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  cargo test --workspace --all-targets --locked
```

See [sandbox setup](boxer.md) if user delegation is unavailable.
Browser tests use the published web build served by the real backend, including
its API, streaming, and WebSocket connections; they do not use Vite as a proxy.
Desktop tests exercise Iced widgets and real
network tasks in its headless runtime. Browser tests use Playwright. iOS tests
run the native SwiftUI client in an iPhone simulator on macOS and exercise
conversation streaming, Profile, Audit, Tasks, and Webhooks against an
in-process HTTP fixture. CI captures the iOS screenshot from the simulator.
On Linux, install `libxkbcommon-dev`, `libwayland-dev`, and
`libfontconfig1-dev` for desktop builds and `bubblewrap` for Linux sandbox tests;
enable unprivileged user namespaces for those isolation tests.
Playwright CI uses `--with-deps`.

Set `SOLMU_CAPTURE_SCREENSHOTS=1` when running Rust and browser e2e tests to
refresh the README screenshots in `docs/screenshots/`.
The CLI test records its real terminal cells into `artifacts/cli.html`;
run `node scripts/capture-cli.mjs` to render it into `docs/screenshots/cli.png`.
The CLI startup chooser is also covered by its startup E2E test.
Muxer tests capture `artifacts/muxer.html`; run
`node scripts/capture-cli.mjs muxer` for `docs/screenshots/muxer.png`.
The native prompt queue test captures `artifacts/muxer-automation.html`;
render it with `node scripts/capture-cli.mjs muxer-automation`.
The direct terminal control test captures `artifacts/muxer-terminal.html`;
render it with `node scripts/capture-cli.mjs muxer-terminal`.
The shell and command test captures `artifacts/muxer-commands.html`;
render it with `node scripts/capture-cli.mjs muxer-commands`.
Skills tests capture `artifacts/cli-skills.html` and `artifacts/muxer-skills.html`;
render them with `node scripts/capture-cli.mjs cli-skills` and
`node scripts/capture-cli.mjs muxer-skills`.
MCP tests capture `artifacts/cli-mcp.html` and `artifacts/muxer-mcp.html`;
render them with `node scripts/capture-cli.mjs cli-mcp` and
`node scripts/capture-cli.mjs muxer-mcp`.
Plugin tests capture `artifacts/cli-plugins.html` and
`artifacts/muxer-plugins.html`; render them with
`node scripts/capture-cli.mjs cli-plugins` and
`node scripts/capture-cli.mjs muxer-plugins`.
Audit tests capture `artifacts/cli-audit.html` and `artifacts/muxer-audit.html`;
render them with `node scripts/capture-cli.mjs cli-audit` and
`node scripts/capture-cli.mjs muxer-audit`.
The Muxer GUI E2E launches a native window under Xvfb without a Muxer process,
sends input to its embedded terminal engine, verifies saved workspace state,
and captures `docs/screenshots/muxer-gui.png` in CI.
Installer tests under `e2e/install` use local release archives and temporary
directories; they do not change your PATH or installed applications. Background
service tests replace OS service commands with recorders, so no real service is
registered or stopped.
Client-specific installer tests live under their respective `e2e/<client>/`
folders. Run all installer tests with
`npx playwright test --config e2e/install/playwright.config.ts`.

GitHub Actions runs formatting, lint, builds, and e2e tests on pushes and pull
requests. Rust is checked on Linux, Windows, and macOS. The website workflow
tests the site before deploying it; the container workflow publishes backend
images to GitHub Container Registry after relevant `main` changes. It starts
the published container and checks thread CRUD and persistence across restart.

## Website documentation

The website's **Docs** section is generated from the Markdown files in this
folder. Add or edit a guide here; it appears in navigation automatically, and
changes on `main` publish through GitHub Pages. The documentation overview is
[index.md](index.md). Keep guide content here as the single source of truth.

Use `npm run dev:website` to preview the website and guides at
`http://127.0.0.1:4174`, or `npm run build:website` to generate `website/dist/`.
The website tests exercise the generated pages under the `/solmu/` project path,
including guide links, section anchors, screenshots, and mobile navigation.
