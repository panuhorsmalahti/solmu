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
npx playwright install chromium
npm run test:e2e
```

Build the binaries before e2e tests. Tests launch a real Solmu backend with
temporary SQLite files and a local provider fixture; they never use your keys
or make paid model requests. Tests live in `e2e/api`, `e2e/cli`, `e2e/desktop`,
`e2e/web`, `e2e/website`, and `e2e/sandbox`.

The CLI tests use a real pseudo-terminal; on Linux they also run the client
inside the isolated sandbox. Linux tests must run in a delegated cgroup v2
hierarchy with CPU, memory, and task controllers:

```sh
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  cargo test --workspace --all-targets --locked
```

See [sandbox setup](sandbox.md) if user delegation is unavailable.
Desktop tests exercise Iced widgets and real
network tasks in its headless runtime. Browser tests use Playwright.
On Linux, install `libxkbcommon-dev`, `libwayland-dev`, and
`libfontconfig1-dev` for desktop builds and `bubblewrap` for Linux sandbox tests;
enable unprivileged user namespaces for those isolation tests.
Playwright CI uses `--with-deps`.

Set `SOLMU_CAPTURE_SCREENSHOTS=1` when running Rust and browser e2e tests to
refresh the README screenshots in `docs/screenshots/`.
The CLI test records its real terminal cells into `artifacts/cli.html`;
run `node scripts/capture-cli.mjs` to render it into `docs/screenshots/cli.png`.
Installer tests under `e2e/install` use local release archives and temporary
directories; they do not change your PATH or installed applications.

GitHub Actions runs formatting, lint, builds, and e2e tests on pushes and pull
requests. Rust is checked on Linux, Windows, and macOS. The website workflow
tests the site before deploying it; the container workflow publishes backend
images to GitHub Container Registry after relevant `main` changes. It starts
the published container and checks thread CRUD and persistence across restart.
