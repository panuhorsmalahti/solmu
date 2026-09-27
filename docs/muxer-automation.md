# Muxer automation

Manage a running local Muxer session from scripts or another terminal. Commands
work while the UI is detached. Start the backend and a session first:

```sh
muxer server start --session work --cwd /path/to/project
muxer api snapshot --session work
muxer space list --session work
muxer pane list --session work
```

Add `--session NAME` before or after any command to select a session; the default
is `default`. Control commands never launch a server or retry a failed mutation.
`muxer api --help` lists commands. `muxer status` is a shortcut for the live snapshot.

## IDs and results

Successful commands print a JSON envelope: `{"ok":true,"result":...}`. Errors go
to stderr as `{"ok":false,"error":"..."}`. Runtime failures exit with code 1;
invalid command arguments exit with code 2. `pane read` prints text by default;
add `--json` for the envelope.

Space, tab, and pane IDs are positive integers local to one session. A space's
first tab and root pane initially share its ID, but each ID refers to its own
kind of item. IDs remain stable through layout changes and server restarts.
Closing an item does not reuse its ID in that saved session. A completely new
session starts a new ID sequence.

`api snapshot` reports the selected space/tab/pane, all spaces, tabs and panes,
attached clients, active monitor count, server PID, protocol version, and installed
Muxer version.
It also reports active native agent requests. Pane records include native
readiness, queued prompt count, and the latest turn when available; see
[Solmu prompt automation](muxer-agents.md).
`pane get ID` includes its working directory, state, native Solmu conversation
ID (`thread`), process ID, and terminal rows/columns. `instance` identifies the
particular pane process and changes when it restarts. A stopped pane remains
inspectable; `exited` contains its reason. A pane still starting may not yet
have a conversation ID. Thread IDs identify conversations in the backend.

## Spaces, tabs, and panes

```sh
muxer space create --cwd /path/to/another-project --name Research
muxer tab create --space 2 --name Planning
muxer pane split 3 --direction right --ratio 0.4 --name Reviewer
muxer pane split 4 --direction down --cwd /path/to/project
muxer tab list --space 2
muxer pane list --tab 3
muxer space get 2
muxer tab get 3
muxer pane get 4
```

Creation returns `space`, `tab`, and `pane` records, including their IDs. Space
and tab creation start a root Solmu pane; a split starts another conversation.
Limits are eight spaces, eight tabs per space, and eight panes per tab.
Paths must be existing directories. The CLI resolves supplied paths relative
to its current directory; omitted paths follow Muxer's [working-directory
setting](muxer-configuration.md). Existing panes keep their working directories.

New items preserve focus by default. `--focus` changes the server's selected
view for the next terminal attachment. `--no-focus` explicitly selects the default.
Existing attached terminals keep their own selected pane and unsaved CLI drafts.

```sh
muxer space rename 2 "My project"
muxer tab rename 3 "Review"
muxer pane rename 4 "Reviewer"
muxer pane rename 4 --clear
muxer pane close 4
muxer tab close 3
muxer space close 2
```

Names accept up to 80 characters without control characters. Names and layouts
are saved immediately and appear in attached UIs automatically. `--clear`
restores the default name. Closing terminates the relevant processes and removes
their layout entries. Project files and saved backend conversations remain.
Closing the final pane stops the session. Use `pane restart ID` only for a stopped
pane: it keeps its layout ID/name and starts a new Solmu conversation and instance.

## Focus and layout

```sh
muxer space focus 2
muxer tab focus 3
muxer pane focus 4
muxer pane swap 3 4
muxer pane zoom 4 on
muxer pane zoom 4 off
muxer pane zoom 4 toggle
muxer pane resize 4 --direction right --ratio 0.6
```

Focus and zoom commands change the server's next-attach view. To change one
already attached terminal, take its ID from `api snapshot`'s `clients` array
and add `--client ID` to a focus or zoom command. Other clients keep their view.
The targeted terminal's view also becomes the next-attach view.

Swap requires two different panes in the same tab and keeps their processes,
conversations, and input drafts intact. Split/resize ratios range from 0.1 to
0.9 and describe the first (left or top) part of a divider. Resize sets the
nearest enclosing divider of that axis; `right` chooses a horizontal split
and `down` a vertical split. Resizing and swaps update the shared layout.
Zoom stays specific to each terminal's view. Closing a pane collapses its split.

## Terminal input and reads

```sh
muxer pane send-text 1 "Review the changes in this workspace"
muxer pane send-keys 1 enter
muxer pane read 1
muxer pane read 1 --json
muxer pane read 1 --lines 10
muxer pane read 1 --ansi
muxer pane send-keys 1 esc
muxer pane send-text 1 /exit
muxer pane send-keys 1 enter
```

`send-text` inserts text into the real pane terminal. It respects bracketed paste
when the application requests it and does **not** submit a message by itself.
The text must contain 1–65536 UTF-8 bytes. Tabs and line breaks require the pane
application to enable bracketed paste; otherwise they are rejected to prevent
accidental submission. Other control characters require `send-keys`.
`send-keys` sends between one and
64 keys in order, such as `enter`, `esc`, `ctrl+u`, `alt+b`, `shift+tab`, arrows,
or `f1`. All keys are validated before sending any input. `prefix+` bindings
are Muxer shortcuts and cannot be used for pane input.
If text looks like an option, place `--` before it, for example
`muxer pane send-text 1 -- --help`.
Place session selectors and other options before that `--` separator.

These are terminal primitives: input goes to whichever CLI screen is open,
including command and Profile screens. An acknowledgement means the bytes were
queued, not that Solmu accepted a message. Read the screen before automating
input. A full input queue or stopped pane returns an error. After a connection
failure, inspect the pane before retrying; input might already have arrived.

Reads show the current visible screen, including stopped panes. Plain text
contains physical rows; `--lines N` keeps the last N viewport rows (1–100),
including empty rows. ANSI reads include cursor/color escape sequences for the
whole viewport and cannot be combined with `--lines`. They do not scroll the
CLI or read older conversation messages from the backend.

## Events and waits

```sh
muxer events
muxer events --pane 1 --count 3 --timeout 60000
muxer pane wait 1 --until idle --until error --timeout 30000
muxer pane wait 1 --until exited --timeout 30000
muxer pane wait-output 1 --match "Hello from Solmu" --timeout 30000
muxer pane wait-output 1 --regex 'Hello\s+from\s+Solmu' --timeout 30000
```

`events` prints one JSON line for an initial `snapshot`, then a `changed` line
whenever Muxer observes a different session snapshot. Each line has a sequence
number starting at 1. Replace your cached snapshot with the latest one. Start
the subscription to obtain the initial state and subsequent updates on one
connection. Reconnect by opening a new subscription, which starts a new sequence
and sends a fresh snapshot. Muxer does not replay earlier events.

`--pane ID` filters a subscription to that pane's metadata. The unfiltered
snapshot includes spaces, tabs, layouts, pane identities/states, and attached
clients. Events describe observed state, rather than every intermediate terminal
write. They do not stream terminal text. A filtered stream follows the pane ID
through restart and reports its new instance; closing that pane ends with an
error. The UI and other control commands remain available during subscriptions.

`--count N` ends a stream after N snapshots, including the initial one (1–100000).
`--timeout MS` ends a stream normally after the specified duration. Omit both to
keep receiving updates until disconnection or session shutdown.

State waits accept `idle`, `working`, `error`, and `exited`; repeat `--until` to
accept any of several states. `working` indicates a model response or pending
stop acknowledgement; background refreshes do not set it. An already matching
state succeeds immediately. Waiting for `idle`
does not submit a message or prove that a particular reply has completed.

Output waits inspect the plain-text visible viewport immediately, then as it
changes. `--match` searches for literal text; `--regex` uses Rust regular
expressions, including flags such as `(?i)` or `(?m)`. Patterns must contain
1–4096 UTF-8 bytes and fit the compiled expression limit. Physical row breaks
remain in the text; use `\s+` to match a wrapped phrase. A successful output wait
returns the pane record, matched text, the inspected screen, and UTF-8 byte
offsets `start`/`end`. It does not scroll or read saved backend history.

Waits pin the pane's instance when accepted. Closing or replacing it fails
instead of allowing another conversation to satisfy the wait. If a pane stops
without matching the output, the output wait fails. `--timeout MS` returns an
error if no match is observed before the deadline; omitting it waits indefinitely.
Timeouts accept 1–86400000 milliseconds and are checked by the server's event
loop. At most 16 waits/subscriptions may be active in one session; bounded
writers prevent a slow subscriber from blocking terminal interaction. Closing
the command releases its monitor automatically.

## Direct requests

Use `muxer api request JSON` to call the same typed control API. Unknown methods,
fields, invalid IDs, and invalid values fail without falling back to another
session. For example:

```sh
muxer api request '{"method":"get","params":{"target":"pane","id":1}}'
muxer api request '{"method":"split_pane","params":{"pane":1,"axis":"right","ratio":400,"name":"Review"}}'
```

Request names are `snapshot`, `list`, `get`, `create_space`, `create_tab`,
`split_pane`, `focus`, `rename`, `close`, `swap`, `zoom`, `resize`, `restart`,
`read`, `send_text`, `send_keys`, `wait`, `wait_output`, and `subscribe`, plus
the native `agent_*` requests below. `snapshot` and `agent_list` have no
`params`; other requests use the following fields. Optional fields may be omitted.

| Request | Required fields | Optional fields |
| --- | --- | --- |
| `list` | `target`: `space`, `tab`, or `pane` | `parent`: space ID for tabs, tab ID for panes |
| `get` | `target`, `id` | |
| `create_space` | | `cwd`, `name`, `focus` (false) |
| `create_tab` | `space` | `cwd`, `name`, `focus` (false) |
| `split_pane` | `pane`, `axis`: `right` or `down` | `ratio` (500), `cwd`, `name`, `focus` (false) |
| `focus` | `target`, `id` | `client` |
| `rename` | `target`, `id`, `name` (string or null to clear) | |
| `close` | `target`, `id` | |
| `swap` | `pane`, `other` | |
| `zoom` | `pane`, `mode`: `on`, `off`, or `toggle` | `client` |
| `resize` | `pane`, `axis`, `ratio` | |
| `restart` | `pane` | |
| `read` | `pane` | `lines`, `ansi` (false) |
| `send_text` | `pane`, `text` | |
| `send_keys` | `pane`, `keys`: array of key strings | |
| `wait` | `pane`, `until`: array of states | `timeout_ms` |
| `wait_output` | `pane`, `pattern` | `regex` (false), `timeout_ms` |
| `subscribe` | | `pane`, `count`, `timeout_ms` |
| `agent_list` | No `params` | |
| `agent_get` | `target`: pane ID or unique name | |
| `agent_prompt` | `target`, `text` | `wait` (false), `timeout_ms` |
| `agent_wait` | `target` | `turn`, `timeout_ms` |
| `agent_turn` | `target`, `turn` | |
| `agent_stop` | `target` | |
| `agent_rename` | `target`, `name` (string or null) | |
| `agent_focus` | `target` | `client` |
| `agent_read` | `target` | `lines`, `ansi` (false) |
| `agent_keys` | `target`, `keys` | |

API ratios are integer thousandths (100–900), unlike the CLI's decimal ratios.
Direct `cwd` paths are interpreted by the server; use absolute paths.

### Local transport

Integrations can use the authenticated localhost TCP connection described by
`<session>.endpoint` in the [Muxer state directory](muxer.md#sessions).
Keep that file and its token private. Each connection carries big-endian u32
length-prefixed UTF-8 JSON frames, capped at 4 MiB. Send one request in its Hello:

```json
{"Hello":{"protocol":1,"token":"TOKEN","operation":"control","width":80,"height":24,"control":{"method":"snapshot"}}}
```

The server replies with `{"Ready":{"pid":123}}`, followed by
`{"Control":RESULT}` or `{"Error":"..."}`, then closes the connection.
Handshake errors may return `Error` without `Ready`. Native agent requests
may return `{"AgentError":FAILURE}` after `Ready`,
where `FAILURE` contains `pane`, `instance`, `thread`, and a nested `failure`
with `code`, `message`, `accepted`, and `turn`.
Authentication reads and bounded reply writers run outside the terminal
coordinator. Subscriptions keep the connection open after `Ready` and receive
`{"Event":EVENT}` frames followed by `{"End":{"reason":"count_reached"}}`
or `{"End":{"reason":"timeout"}}` when a limit is reached. Waits keep the
connection open until their `Control` result or `Error`. A connection owns
one request; it cannot send additional mutations after Hello. Attach clients
and control clients share the session, with separate view state.

![Solmu Muxer](screenshots/muxer.png)

[Direct terminal attachment](muxer-terminals.md) opens one existing pane or streams its
live screen to scripts. Several observers can watch while one controller owns
input and size, with explicit takeover and draft-preserving detach.

[Shell and command panes](muxer-commands.md) run local terminals and project commands beside
Solmu. New-pane defaults apply automatically; saved commands wait for explicit
restart after the server restarts.
