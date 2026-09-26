# Desktop client

The desktop uses the web client's light background, forest green controls,
soft conversation cards, and highlighted thread selection.

Start the backend, then run `cargo run -p solmu-desktop` from the repository
root. The app uses `SOLMU_BACKEND_URL` (default `http://127.0.0.1:3000`).

The left sidebar lists saved conversations. Click one to load its history;
click **+** to create a new thread. A new thread is also created on startup.
Edit the title and click **Rename** to save it. **Delete** removes the selected
thread and all its messages. Changes from other clients appear automatically
over WebSockets. New threads are named after the first user message.

Type into the message input and press Enter or **Send**. Replies stream into
the conversation and are saved when complete. **Stop** cancels a response and
discards the unfinished reply, keeping your user message. Wait for completion before
changing threads or sending again. Provider errors appear above the input;
the user message is retained. Closing the window exits without deleting history.

![Solmu client screenshot](screenshots/desktop.png)
