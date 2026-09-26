# Solmu contributor instructions

All user-facing features must have a corresponding end-to-end test.

All user-facing changes and features must be documented in the root `docs/` folder.

Scope end-to-end tests by client under `e2e/<client>/`, for example `e2e/cli/`.
Use `e2e/api/` for tests of the backend HTTP API.

Commit automatically after each major change.

All clients must stay feature complete, with full end-to-end coverage at all times.
Keep supported conversation features available in CLI, desktop, and web; use UI
patterns suited to each client (CLI commands; desktop/web sidebar controls).

The website must remain feature complete as a concise introduction to Solmu.
Describe every relevant higher-level user feature and keep its client and setup
links current. Do not advertise capabilities that are not implemented.

The agent system prompt must mention relevant new capabilities when needed.
Review and update it with feature changes; keep it concise.
