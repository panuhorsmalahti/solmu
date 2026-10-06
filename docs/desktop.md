# Desktop client

The desktop uses the web client's light background, forest green controls,
soft conversation cards, and highlighted thread selection.

With the backend already running, run the installed `solmu-desktop` command.
See [installation](../clients/desktop/README.md#install). The app uses `SOLMU_BACKEND_URL`
(default `http://127.0.0.1:3000`).

The left sidebar lists saved conversations. Click one to load its history;
click **+** to create a new thread. A new thread is also created on startup.
Edit the title and click **Rename** to save it. **Delete** removes the selected
thread and all its messages. Changes from other clients appear automatically
over WebSockets. New threads are named after the first user message.

Repeated **+** clicks reuse the current empty conversation until you send a
message. **Profile**, above Conversations, edits your shared system prompt and
optional default model. It shows the prompt's last edit time and the actual
backend default in the empty Model field. Changes appear live; unsaved edits
are preserved. The conversation's model button opens known OpenAI or Anthropic
choices, or a custom model ID. Default removes the thread override. The working
folder is shown below the conversation title. See [Profile](profile.md) and
[workspaces](workspaces.md).

Type into the message input and press Enter or **Send**. Replies stream into
the conversation and are saved when complete. **Stop** cancels a response and
discards the unfinished reply, keeping your user message. Wait for completion before
changing threads or sending again. Provider errors appear above the input;
the user message is retained. Closing the window exits without deleting history.

Choose **Compact** in the conversation controls to replace the visible history
with a continuation summary. Solmu also compacts automatically at an estimated
95% of the selected model's context window.

Use **Status** to see the backend connection, thread, model, and workspace.
**Context** summarizes the messages, tool calls, skills, MCP servers, and
plugins currently loaded for the conversation. **Export** saves the thread as
Markdown to the path shown in its panel. **Copy reply** copies Solmu's latest
reply to the system clipboard.

Type `/` in the chat input to browse and filter slash commands, then select a
suggestion to insert it into the input. Run it with Enter or **Send**. Available
commands are `/new`, `/threads`, `/open`, `/model`, `/profile`, `/audit`,
`/tasks`, `/task`, `/skills`, `/mcp`, `/plugins`, `/rename`, `/delete`,
`/status`, `/export`, `/copy`, `/context`, `/compact`, `/stop`, and `/help`.

![Solmu client screenshot](screenshots/desktop.png)

## Audit

Choose **Audit** below Profile to browse tool calls across conversations.
Click a call for arguments and results; **Previous** and **Next** change pages.
The list updates automatically. See [Audit](audit.md).
The Audit header also shows the last 24 hours' prompt cache hit rate and token counts.

![Desktop Audit](screenshots/desktop-audit.png)

## Scheduled tasks

Enter `/goal <objective>` in chat to start a persistent goal, or `/goal` to
list goals across conversations. See [Goals](goals.md).

Choose **Tasks** in the sidebar to create one-time or recurring work, edit or
pause it, run it now, view run history, and open its conversation. Task changes
appear automatically. See [scheduled tasks](tasks.md).

![Desktop tasks](screenshots/desktop-tasks.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](tools.md) for usage and limits.

## Skills

Choose **Skills** in the conversation controls to see the workspace's installed
[skills](skills.md). The list updates automatically and preserves your draft.

## MCP

Open **MCP** in a conversation to see live server status and tools while keeping your unsent draft. See [MCP setup](mcp.md).

![Desktop MCP status](screenshots/desktop-mcp.png)

## Plugins

Open **Plugins** in a conversation to inspect installed [Agent Plugins](plugins.md)
and loading errors. Your unsent draft is preserved and the list updates automatically.

![Desktop plugins](screenshots/desktop-plugins.png)
