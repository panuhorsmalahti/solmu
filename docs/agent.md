# Solmu agent

Solmu uses two system prompts for every reply:

- **Internal:** built-in instructions introduce Solmu, describe workspace tools,
  require respecting filesystem and network restrictions,
  and explain installing skills in `.agents/skills/<name>/SKILL.md`, reading
  relevant instructions, and resolving resources. Profile cannot replace this
  prompt; it is maintained with the backend.
- **Editable:** your saved Profile prompt supplies preferences such as language,
  tone, and project guidance. Its initial value is
  “You are Solmu, an autonomous agent.”

The internal prompt comes first, then your editable prompt. The full saved
conversation and tool results follow them. Solmu can
manage conversations, stream replies, and use [tools](tools.md) to work in a
[workspace](workspaces.md). Edit your preferences in [Profile](profile.md); upgrades
preserve your saved prompt.

The built-in `Memory` tool lets Solmu recall durable facts across conversations.
It saves, updates, or deletes facts only when you ask or clearly expect it, and
adds up to five matching memories to a reply's context. Browse the read-only
list in [Memories](memories.md), or ask Solmu to remember or change a fact.

When you ask Solmu to schedule work, it can create or manage
[scheduled tasks](tasks.md). Each run works in a saved conversation.

Installed [workspace skills](skills.md) enter the runtime context automatically.
Solmu reads relevant instructions and supporting resources as needed.

You can stop a reply while waiting for it. The user message remains saved;
an interrupted assistant reply is discarded. Automatic title generation can
still finish after stopping a reply. Completed tool changes and their saved
results remain available; Stop cancels pending tool calls.

## MCP tools

Solmu connects to trusted workspace MCP servers automatically. Connected tools become available to the agent on each reply and their results appear in the conversation. Inspect status with `/mcp` or the **MCP** panel. See [MCP setup](mcp.md).

Solmu also discovers [Agent Plugins](plugins.md) in `.agents/plugins/`. A plugin can
provide skills and MCP tools. Inspect installed packages with `/plugins` or the
**Plugins** panel.
