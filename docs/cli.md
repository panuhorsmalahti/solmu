# Terminal client

If a conversation operation or live refresh is still finishing when you press
Enter, the CLI keeps the submission and sends it once that operation completes.

With the backend already running, run the installed `solmu` command from the
folder you want to work in. See [installation](../clients/cli/README.md#install).
The CLI connects to `SOLMU_BACKEND_URL`, defaulting to `http://127.0.0.1:3000`,
and creates a new conversation by default. To reopen a saved conversation at
startup, use `solmu --thread <id>`. This does not create another thread.
`solmu --help` shows startup options.

Type a message and press Enter to receive a streamed reply. Conversations and
completed replies are saved by the backend. Use PageUp/PageDown to scroll.
Ctrl+L redraws the terminal without changing your draft or conversation.
An animated spinner shows progress. Tab completes a slash command prefix;
repeated Tab cycles through matches. For example, `/ren` + Tab completes `/rename`.
Typing `/` opens a filtered command list. Up/Down chooses a command;
Tab completes it and Enter selects it.

Use `/threads` to list conversations and `/open <id>` to continue one.
`/new [title]` creates a conversation, `/rename <title>` changes its title,
and `/delete` deletes it and its messages. `/help` shows commands.
Conversation changes from other clients appear automatically over WebSockets.
New threads are named automatically after the first user message.
`/exit` or Ctrl+C exits without deleting conversations.

Use `/compact` to replace the visible conversation history with a continuation
summary. Solmu also compacts automatically when estimated context use reaches
95% of the model's window.

Use `/status` to see the backend connection, selected thread, model, and
workspace. `/context` summarizes the loaded conversation and workspace tools.
`/export <path>` saves the conversation as Markdown, and `/copy` copies the
latest Solmu reply through the terminal's clipboard support (OSC 52).

`/new` reuses the current empty conversation until you send a message.
`/profile` edits the shared system prompt and optional default model, with its
last edit time shown. `/model` opens the thread model picker, `/model <id>` sets
a custom model, and `/model default` clears the thread override. OpenAI and
Anthropic show named model choices. Each new CLI thread uses your current
working directory on the backend. See [Profile](profile.md) and
[workspaces](workspaces.md).

Provider errors appear above the input. The user message is retained; partial
assistant replies are not saved. Wait for a reply to finish before another
message or conversation change. Press Esc or use `/stop` to cancel a response;
the saved user message remains, and the unfinished reply is discarded.
You can exit during a reply.

See the [CLI README](../clients/cli/README.md) for the full setup.

![Solmu client screenshot](screenshots/cli.png)

## Audit

`/audit` opens saved tool calls across all conversations, newest first. Use
Up/Down to select, Enter to expand details, PageUp/PageDown for older pages,
and Esc to return. See [Audit](audit.md).
The Audit header also shows the last 24 hours' prompt cache hit rate and token counts.

![CLI Audit](screenshots/cli-audit.png)

## Memories

Use `/memories` to browse shared saved facts, newest first. PageDown loads older
entries. See [Memories](memories.md).

![CLI Memories](screenshots/cli-memories.png)

## Scheduled tasks

Use `/goal <objective>` to start a persistent multi-step goal, or `/goal` to
list goals across conversations. See [Goals](goals.md).

Use `/tasks` to browse scheduled work. `/task once <RFC3339 time> | <name> | <prompt>`
creates a one-time task; `/task cron <five fields> | <name> | <prompt>` creates a
recurring task. `/task run`, `pause`, `resume`, `runs`, and `delete` take a task ID.
Use `/task edit <id> | <name> | <prompt> | <schedule> | <once|cron>` to change it.
See [scheduled tasks](tasks.md).

![CLI tasks](screenshots/cli-tasks.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](tools.md) for usage and limits.

## Skills

`/skills` lists the current workspace's automatically discovered
[skills](skills.md). Esc returns to your conversation.

## MCP

`/mcp` lists each configured workspace server, connection status, and available tools. The list updates automatically. See [MCP setup](mcp.md).

![CLI MCP status](screenshots/cli-mcp.png)

## Plugins

`/plugins` lists installed [Agent Plugins](plugins.md), their skills and MCP
servers, and any loading errors. The list updates automatically.

![CLI plugins](screenshots/cli-plugins.png)
