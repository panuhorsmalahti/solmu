# Workspace skills

Skills give Solmu reusable instructions for work in a project. Install them in
the conversation's workspace, inside `.agents/skills/`:

```text
your-project/
└── .agents/
    └── skills/
        └── clear-writing/
            ├── SKILL.md
            └── references/       # optional
```

Each skill needs a `SKILL.md` file with YAML metadata and Markdown instructions:

```markdown
---
name: clear-writing
description: Write clear project documentation. Use when explaining a feature or editing a guide.
---

Use short sentences and concrete examples.
Explain what the reader can do before describing implementation details.
```

The name must match the containing folder. Use 1–64 lowercase letters, numbers,
and single hyphens, without a leading or trailing hyphen. Descriptions must
contain 1–1024 characters. The optional `license`, `compatibility`, `metadata`,
and `allowed-tools` fields follow the
[Agent Skills specification](https://agentskills.io/specification).

## Use installed skills

No registration or backend restart is needed. Solmu discovers valid skill folders
automatically before each reply and adds their names, descriptions, and file
locations to its context. It reads a relevant skill's full `SKILL.md` with Read
before applying its instructions. Referenced files are read when needed, relative
to the skill's folder. Scripts are available to existing tools; installing a
skill does not execute its scripts or grant additional tool permissions.

This follows the format's progressive loading: the catalog enters context
automatically; instructions and resources are loaded as the task needs them.
Catalog context is runtime information, separate from saved chat messages and
your editable Profile prompt. Built-in skill installation and usage instructions
live in the internal system prompt, which remains present when you edit Profile.
Upgrades preserve your existing editable prompt.

The CLI uses its thread's working folder, usually the folder where you ran
`solmu`. Desktop and web use the thread's configured [workspace](workspaces.md).
Skills from another project are not included. Files in a plain `skills/` folder
or another repository are not scanned.
Skills bundled in [Agent Plugins](plugins.md) under `.agents/plugins/` are also
listed here and enter the agent context automatically.

Installed skill directory links are supported. Read, Glob, and Grep can access
resources inside those installed skill directories, including directories linked
outside the workspace. Write and Edit retain their workspace boundary. Resource
links that escape the skill directory are not included in its read access.

## See available skills

- **CLI:** `/skills` lists discovered names, descriptions, and paths. Tab completes
  the command; Esc returns to the conversation.
- **Desktop and web:** choose **Skills** in the conversation controls, then
  **Back to conversation** to return. Your unsent message stays intact.
- **Muxer:** use `/skills` inside a Solmu pane.

Open lists update automatically as skills are installed, edited, or removed.
The backend checks watched catalogs once per second and sends WebSocket updates.
Reconnecting also fetches the current catalog. An already running reply keeps
the catalog it started with; the next reply discovers changes again.

Malformed skills appear under **Not loaded**, with the reason. Other valid
skills remain available. A workspace without installed skills has an empty list.
Solmu loads at most 128 valid skills in directory-name order, and each `SKILL.md`
must be UTF-8 text no larger than 1 MB. Instructions may span multiple Read calls.

## API

`GET /api/v1/threads/{id}/skills` returns `directory`, `items`, and `issues`.
Each item includes `name`, `description`, `path`, optional metadata, and
compatibility requirements. Each issue includes `path` and `message`.
An unknown thread returns 404. This endpoint lists skills without sending a
message or calling an LLM provider.

`{ "type": "skills_changed", "thread_id": "..." }` on `/api/v1/events`
means fetch that thread's catalog again.

## Screenshots

![CLI workspace skills](screenshots/cli-skills.png)

![Desktop workspace skills](screenshots/desktop-skills.png)

![Web workspace skills](screenshots/web-skills.png)

![Skills in a Muxer Solmu pane](screenshots/muxer-skills.png)
