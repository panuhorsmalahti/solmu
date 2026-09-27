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

![Solmu client screenshot](screenshots/desktop.png)

## Workspace tools

Solmu can read, edit, and search files and run Bash commands in the thread's
workspace. Tool activity and results appear live and stay in conversation history.
Stop cancels pending work. See [tools](tools.md) for usage and limits.

## Skills

Choose **Skills** in the conversation controls to see the workspace's installed
[skills](skills.md). The list updates automatically and preserves your draft.
