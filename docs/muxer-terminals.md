# Attach directly to a Muxer pane

Open one running Solmu terminal without the spaces and tabs around it. The
conversation, current screen, and unsent draft belong to the existing pane.
The backend and Muxer session must already be running.

```sh
muxer pane list --session work
muxer terminal attach 1 --session work
```

Targets are numeric pane IDs or exact, unique live pane names. You can also use
`muxer agent attach Planner --session work`. A direct attachment never starts
another conversation or switches another Muxer client's selected tab.

Use the usual Solmu commands, including `/profile`, `/model`, and `/stop`.
Press **Ctrl+b**, then **q** to detach. The pane keeps running and its draft
is preserved. **Ctrl+b Ctrl+b** sends a literal Ctrl+b. Ctrl+C and `/exit`
reach Solmu and exit that pane's process.

## One controller, several observers

A direct controller owns terminal input and dimensions until it detaches,
disconnects, or is replaced. Another controller is refused unless it explicitly
uses `--takeover`; the old controller receives a `taken_over` close reason.

```sh
muxer terminal attach Planner --takeover --session work
```

Ordinary Muxer clients can still view the pane and manage layouts. Typing into a
controlled pane shows a notice; terminal input automation is also refused.
Other panes remain usable. Native [prompt queues](muxer-agents.md) remain
available because they address Solmu directly rather than typing into its terminal.
After release, Muxer controls input and size again.

![Direct terminal control in Muxer](screenshots/muxer-terminal.png)

## Script streams

Observe without input, resize, scroll, or takeover authority:

```sh
muxer terminal session observe Planner --session work
```

Observers can choose their own clipped viewport with `--cols` and `--rows`.
This never changes the running pane's dimensions or another observer's view.
Several observers can remain connected while one controller works.

For a writable stream, keep stdin open and write one JSON command per line:

```sh
muxer terminal session control Planner --cols 120 --rows 40 --session work
```

Supported stdin commands:

```json
{"type":"input","data":"SGVsbG8="}
{"type":"resize","cols":120,"rows":40}
{"type":"scroll","lines":20}
{"type":"release"}
```

`data` is base64-encoded raw terminal input; `SGVsbG8=` types `Hello` without
submitting. Encode Enter as `\r` when you intend to submit. Positive scroll
counts move up in terminal scrollback; negative counts move toward the bottom.
This is terminal scrollback, separate from Solmu's PageUp/PageDown conversation
scrolling. The terminal may have no scrollback while Solmu uses its full screen.
EOF on controller stdin releases control. Observer mode does not read stdin.

Both modes print newline-delimited records on stdout, without the ordinary
automation result envelope:

```json
{"type":"frame","pane":1,"instance":"process-uuid","cols":120,"rows":40,"bracketed_paste":false,"data":"base64-ansi-screen"}
{"type":"error","message":"Terminal input must be base64"}
{"type":"closed","reason":"released"}
```

A frame is a complete ANSI screen snapshot, including colors, cursor, and
terminal input modes. Frames arrive on changes; a quiet pane stays connected.
The initial snapshot is sent immediately. Every frame identifies the same pane
process instance. Exit, closure, restart, or server shutdown ends that stream;
it never follows a replacement process. Reconnect explicitly after checking the
pane's current identity. A malformed script command closes its connection.
Input/resize/scroll validation failures emit an error record without applying
that command. Startup errors go to stderr and exit with code 1; CLI argument
errors exit with code 2.

Dimensions are limited to 240 columns and 100 rows. Input is limited to 64 KiB
per command; a JSON command line to 128 KiB; a scroll command to 2000 rows.
A session accepts 16 direct attachments. Bounded queues and socket timeouts
disconnect stalled readers while other panes and clients keep working. A stalled
stream can end without a final close record.

The [raw automation API](muxer-automation.md) opens a JSON stream with
`{"method":"terminal_open","params":{"target":1,"observe":true}}`.
Use `observe:false` for control; optional fields are `takeover`, `cols`, and `rows`.
These commands require the local session's existing authentication.
