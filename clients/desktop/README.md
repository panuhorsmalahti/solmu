# Solmu desktop

A native Rust desktop client built with Iced.

## Run

Start the backend from the repository root with `cargo run -p solmu-backend`.
Then run in another terminal:

```sh
cargo run -p solmu-desktop
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

See [desktop usage](../../docs/desktop.md) and [configuration](../../docs/configuration.md).

![Solmu client screenshot](../../docs/screenshots/desktop.png)
