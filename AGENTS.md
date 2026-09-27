# Solmu contributor instructions

All user-facing features must have a corresponding end-to-end test.

All user-facing changes and features must be documented in the root `docs/` folder.

Whenever a client's UI changes, update its screenshot in `docs/screenshots/`.
Keep the screenshot links in client guides, READMEs, and the website current.

Do not add reload or refresh buttons anywhere. Every web page must remain
reactive and refresh automatically when relevant data changes, including after
reconnecting. Preserve unsaved user edits during automatic updates.

Scope end-to-end tests by client under `e2e/<client>/`, for example `e2e/cli/`.
Use `e2e/api/` for tests of the backend HTTP API.

Commit automatically after each major change.

All clients must stay feature complete, with full end-to-end coverage at all times.
Keep supported conversation features available in CLI, desktop, and web; use UI
patterns suited to each client (CLI commands; desktop/web sidebar controls).

The website must remain feature complete as a concise introduction to Solmu.
Describe every relevant higher-level user feature and keep its client and setup
links current. Do not advertise capabilities that are not implemented.

The agent's internal system prompt must mention relevant new capabilities when needed.
Review and update it with feature changes; keep it concise.
Keep built-in operating instructions in the internal prompt, separate from the
user-editable Profile prompt. Preserve users' saved prompts during upgrades.
