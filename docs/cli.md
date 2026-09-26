# Terminal client

If a conversation operation or live refresh is still finishing when you press
Enter, the CLI keeps the submission and sends it once that operation completes.

Start the backend, then run `cargo run -p solmu-cli` from the repository root.
The CLI connects to `SOLMU_BACKEND_URL`, defaulting to `http://127.0.0.1:3000`,
and creates a new conversation every time it opens.

Type a message and press Enter to receive a streamed reply. Conversations and
completed replies are saved by the backend. Use PageUp/PageDown to scroll.
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

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](tools.md) for usage and limits.
