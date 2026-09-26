# Terminal client

Start the backend, then run `cargo run -p solmu-cli` from the repository root.
The CLI connects to `SOLMU_BACKEND_URL`, defaulting to `http://127.0.0.1:3000`,
and creates a new conversation every time it opens.

Type a message and press Enter to receive a streamed reply. Conversations and
completed replies are saved by the backend. Use PageUp/PageDown to scroll.
An animated spinner shows progress. Tab completes a slash command prefix;
repeated Tab cycles through matches. For example, `/ren` + Tab completes `/rename`.

Use `/threads` to list conversations and `/open <id>` to continue one.
`/new [title]` creates a conversation, `/rename <title>` changes its title,
and `/delete` deletes it and its messages. `/help` shows commands.
`/exit` or Ctrl+C exits without deleting conversations.

Provider errors appear above the input. The user message is retained; partial
assistant replies are not saved. Wait for a reply to finish before another
message or conversation change. You can exit during a reply.

See the [CLI README](../clients/cli/README.md) for the full setup.
