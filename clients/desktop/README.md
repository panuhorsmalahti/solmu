# Solmu desktop

A native Rust desktop client built with Iced.

## Run

After using the [install script](../../docs/releases.md#install), create `.env`
in your working directory with your provider credentials. Start the installed backend:

```sh
solmu-backend
```

In another terminal:

```sh
solmu-desktop
```

Fill provider credentials in the backend's `.env`. The desktop client connects
to `http://127.0.0.1:3000`, or the address in `SOLMU_BACKEND_URL`.

## Use

The app creates a new conversation on startup. Use the **+** icon in the left
sidebar for another thread, or select an existing thread to resume its history.
Edit the title and click **Rename**, or **Delete** to remove a conversation.
Type a message and press Enter or **Send**; replies appear as they stream.
Use **Stop** while waiting to cancel a response. Conversations update live
when another client changes them. Errors appear above
the message input. Close the window to exit; conversations remain saved.

Open **Profile** above Conversations to edit the system prompt and optional
default model, and see when the prompt was last edited. An empty Model field
shows the backend default. Use the conversation's model button to pick a model
for that thread. Repeated **+** clicks reuse the empty thread until you send a
message. See [Profile](../../docs/profile.md) and [workspaces](../../docs/workspaces.md).

See [desktop usage](../../docs/desktop.md) and [configuration](../../docs/configuration.md).
For source builds, see [development](../../docs/development.md).

![Solmu client screenshot](../../docs/screenshots/desktop.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](../../docs/tools.md) for usage and limits.
