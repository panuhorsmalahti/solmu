# Solmu CLI

A Rust terminal interface for Solmu. Launch into a new or existing saved conversation.

## Run

After using the [install script](../../docs/releases.md#install), create `.env`
in your working directory and fill `OPENAI_API_KEY`. Start the installed backend:

```sh
solmu-backend
```

In another terminal:

```sh
solmu-cli
```

The default backend is `http://127.0.0.1:3000`. Set `SOLMU_BACKEND_URL` in `.env`
or your shell to connect elsewhere. Provider keys belong to the backend.

Use `solmu-cli --thread <id>` to reopen a conversation without creating another.
Find IDs with `/threads`.
`solmu-cli --help` shows startup options; `--version` shows the installed version.

## Use

Type a message and press Enter. Replies appear as they stream and are saved
when complete. PageUp and PageDown scroll the conversation.
Ctrl+L redraws the screen and preserves your unsent input.
An animated spinner indicates that Solmu is working. Type a command prefix
such as `/ren` and press Tab to complete it. Repeated Tab cycles matching commands.
Typing `/` shows the command list. Use Up/Down to choose and Enter to select.

| Command | Action |
| --- | --- |
| `/new [title]` | Start another conversation. |
| `/threads` | List saved conversations with their IDs. |
| `/open <id>` | Open a saved conversation and its history. |
| `/profile` | Edit the shared system prompt and optional default model; see its last edit time. |
| `/model [id\|default]` | Pick a thread model, set an ID, or restore the default. |
| `/rename <title>` | Rename the current conversation. |
| `/delete` | Delete the current conversation and its messages. |
| `/help` | Show commands. |
| `/stop` | Cancel the current response. Esc also stops it. |
| `/exit` | Exit. Ctrl+C also exits. |

Wait for a reply to finish before sending again or changing conversations.
`/exit` still works during streaming. Errors are shown above the input; user
messages remain saved after provider failures or cancellation. Conversations
update automatically when another client changes them.

New threads use your current working folder. `/new` reuses an empty thread until
you send a message. In Profile, Tab switches fields, Ctrl+S saves, Ctrl+U clears,
and Esc returns. The empty Model field shows the actual backend default.

See [CLI usage](../../docs/cli.md), [configuration](../../docs/configuration.md),
and [Docker instructions](../../docs/running.md).
For source builds, see [development](../../docs/development.md).

![Solmu client screenshot](../../docs/screenshots/cli.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](../../docs/tools.md) for usage and limits.
