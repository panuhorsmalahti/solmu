# Automating Solmu in Muxer

Send messages to a running Solmu pane from scripts, even while Muxer is
detached. Start the backend and a local session first. Use matching current
Solmu CLI and Muxer binaries.

```sh
muxer server start --session work --cwd /path/to/project
muxer agent list --session work
muxer agent rename 1 Writer --session work
muxer agent prompt Writer "Review the project" --wait --timeout 60000 --session work
```

## Choosing a pane

`agent list` lists live Solmu panes. `agent get TARGET` returns one pane,
including its workspace, conversation, process instance, and native readiness.
Targets are a numeric pane ID or an exact, unique pane name. Agent names use
the same persistent labels as pane menus; they are separate from conversation
titles. `agent rename TARGET NAME` rejects a duplicate live name, and
`agent rename TARGET --clear` restores the default label.
The CLI interprets digits as pane IDs; use the pane ID for a numeric-only label.

If ordinary pane naming creates duplicates, commands using that name fail as
ambiguous. Use a numeric ID or rename the panes. A stopped pane is available
through `pane get`, but cannot receive agent commands.

`native` is null while controls initialize or if an older CLI lacks support.
Once initialized, `native.ready` means the conversation is available and no
operation or queued prompt is pending. `native.queued` counts unsent prompts;
pane headers display this count. `native.turn` identifies the latest turn and
its state. These fields also update in [session event streams](muxer-automation.md#events-and-waits).

## Sending and waiting

```sh
muxer agent prompt Writer "First task"
muxer agent prompt Writer "Second task" --wait --timeout 60000
muxer agent wait Writer --turn TURN_ID --timeout 60000
muxer agent turn Writer TURN_ID
muxer agent stop Writer
```

`agent prompt` submits message text directly through Solmu's conversation API.
It preserves terminal drafts and unsaved Profile edits. Text beginning with `/`
is a message, not a CLI command. Multi-line text is supported; prompts must be
nonempty, at most 65536 UTF-8 bytes, and contain no control characters except
tabs and line breaks. Use `--` before text that resembles an option; put session
selectors and other options before that separator.

Without `--wait`, success acknowledges an accepted prompt and returns its
unique `turn.id`. The initial state is `queued`. Prompts are sent in order as
the CLI becomes available; up to sixteen unsent prompts can wait behind an
active response or conversation operation. A full queue rejects new work.
Queued messages are not saved until submission begins.

`--wait` succeeds only when that specific turn saves its assistant reply.
Earlier replies, idle periods, and automatic refreshes cannot satisfy it.
`agent wait --turn ID` waits for the same result later. Without a turn ID,
`agent wait` waits for the latest turn in the current conversation; if none
exists, it waits for readiness. Omitting `--timeout` waits indefinitely.
Timeouts accept 1 through 86400000 milliseconds.

Turn states are `queued`, `working`, `succeeded`, `stopped`, and `failed`.
`agent turn` returns the retained record in any state. A completed record
includes saved user and assistant message IDs and a reply preview capped at
65536 UTF-8 bytes; `truncated` indicates whether it was shortened. Full messages
remain in backend history. The CLI retains up to 64 turn records in memory;
unfinished turns are kept. Records expire as new turns replace older completed
records, and disappear when that CLI process exits.

`agent stop` cancels the active reply and all unsent prompts. It acknowledges
the stop request; use `agent wait --turn ID` to observe the outcome. Stopped or
failed turns make waits fail, while `agent turn` still returns their records.
Esc and `/stop` inside the conversation also clear the queue. Switching the
CLI to another conversation fails pending work tied to the old thread;
prompts are never redirected to the new conversation. Closing or restarting
the pane also fails outstanding waits.

## Errors and retries

Successful commands print `{"ok":true,"result":...}`. Native failures print
JSON to stderr with `ok`, `error`, `code`, `accepted`, `turn`, `pane`,
`instance`, and `thread`. Runtime failures exit with code 1; invalid command
arguments exit with code 2.

`accepted: true` means Solmu accepted the identified turn, even when a wait
times out or its process disappears. `false` means the request was rejected
before acceptance. `null` means acceptance could not be established, such as
a connection breaking before its acknowledgement. Inspect the pane and saved
history before retrying an uncertain submission. Commands never retry it
automatically. A timeout or disconnected waiting command does not cancel
accepted work; use `agent stop` explicitly.

Each request pins the original pane process and conversation. Retained turn
IDs from another conversation or a restarted process return `turn_unknown`.
There are at most sixteen active native requests per Muxer session. Closing a
waiting command releases its worker without stopping accepted messages.
Ordinary terminal controls remain available during native waits.

## Terminal controls

```sh
muxer agent focus Writer --client CLIENT_ID
muxer agent read Writer
muxer agent read Writer --json
muxer agent send-keys Writer esc
```

These are name-aware shortcuts for [pane focus, reads, and key input](muxer-automation.md).
They still operate on the actual CLI screen. In particular, `send-keys esc`
may close an open Profile page, whereas `agent stop` addresses pending replies
directly. Focus without `--client` changes the next-attach view; a client ID
selects one attached terminal.

![Solmu Muxer with queued prompts and an unsent terminal draft](screenshots/muxer-automation.png)

[Direct terminal attachment](muxer-terminals.md) opens one existing pane or streams its
live screen to scripts. Several observers can watch while one controller owns
input and size, with explicit takeover and draft-preserving detach.
