# Solmu CLI

A Rust terminal interface for Solmu. Every launch creates a new saved conversation.

## Run

From the repository root, fill `OPENAI_API_KEY` in `.env`, then start the backend:

```sh
cargo run -p solmu-backend
```

In another terminal:

```sh
cargo run -p solmu-cli
```

The default backend is `http://127.0.0.1:3000`. Set `SOLMU_BACKEND_URL` in `.env`
or your shell to connect elsewhere. Provider keys belong to the backend.

## Use

Type a message and press Enter. Replies appear as they stream and are saved
when complete. PageUp and PageDown scroll the conversation.

| Command | Action |
| --- | --- |
| `/new [title]` | Start another conversation. |
| `/threads` | List saved conversations with their IDs. |
| `/open <id>` | Open a saved conversation and its history. |
| `/rename <title>` | Rename the current conversation. |
| `/delete` | Delete the current conversation and its messages. |
| `/help` | Show commands. |
| `/exit` | Exit. Ctrl+C also exits. |

Wait for a reply to finish before sending again or changing conversations.
`/exit` still works during streaming. Errors are shown above the input; user
messages remain saved after provider failures.

See [CLI usage](../../docs/cli.md), [configuration](../../docs/configuration.md),
and [Docker instructions](../../docs/running.md).
