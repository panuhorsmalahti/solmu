# Scheduled tasks

Ask Solmu to do work later, or create a task from a client. Tasks can run once at
a specified time or repeatedly on a five-field cron schedule. The backend must
remain running for tasks to run. Scheduled times use UTC for cron; one-time
times accept an explicit time zone offset.

Each task has a dedicated conversation. The prompt, replies, and tool calls are
saved there, so you can open the conversation to see what happened. **Run history**
shows when each attempt started and whether it completed or failed. A scheduled
run uses the task's workspace and Solmu's configured model and tools. The agent
can also create, list, edit, pause, resume, run, and remove tasks when you ask it
to do so.

Tasks created in the CLI use its current working folder. Tasks created in web
or desktop use Solmu's default workspace (`~/.solmu`, or the equivalent on
Windows). Tasks created by asking the agent use that conversation's workspace.

## Manage tasks

- **Web:** open **Tasks** in the sidebar or `/tasks`. Create or edit a task, run
  it now, pause or resume it, view run history, or open its conversation.
- **Desktop:** choose **Tasks** in the sidebar for the same controls. Enter a
  one-time date as RFC 3339, for example `2026-10-01T09:00:00Z`, or a cron
  expression such as `0 9 * * *`.
- **CLI and Muxer:** use `/tasks` to browse, then `/task` commands to manage
  tasks. For example:

  ```text
  /task once 2026-10-01T09:00:00Z | Morning review | Summarize the workspace
  /task cron 0 9 * * * | Daily review | Review new changes
  /task run <id>
  /task pause <id>
  /task resume <id>
  /task runs <id>
  /task edit <id> | New name | New prompt | 0 10 * * * | cron
  /task delete <id>
  ```

A cron expression has minute, hour, day of month, month, and day of week, in
that order. `0 9 * * *` runs at 09:00 UTC each day. Pausing keeps the task and
its history. Deleting a task keeps its conversation. A one-time task is disabled
after it is claimed; **Run now** still works for a disabled task.

If the backend is offline at a scheduled time, an overdue task runs once after
restart. Earlier cron occurrences are not replayed; the next future occurrence
is scheduled. Tasks
that were interrupted by shutdown are marked interrupted in their run history.

![Tasks in the CLI](screenshots/cli-tasks.png)

![Tasks in the desktop app](screenshots/desktop-tasks.png)

![Tasks on the web](screenshots/web-tasks.png)

![Tasks in Muxer](screenshots/muxer-tasks.png)
