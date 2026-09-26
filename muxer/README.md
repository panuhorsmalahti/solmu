# Solmu muxer

A Rust terminal workspace for running several Solmu CLI sessions at once.
Each pane owns a real terminal and creates its own conversation on startup.

```sh
cargo build -p solmu-cli -p solmu-muxer
cargo run -p solmu-backend
# In another terminal:
cargo run -p solmu-muxer
```

Press **Ctrl+b**, then **n** for a new pane, **Tab** to switch, **s** for a
split view, or **q** to quit. Click a pane or sidebar entry to focus it.
All Solmu conversation commands work inside panes, including `/stop` and `/exit`.

Run `muxer --help` or read the [muxer guide](../docs/muxer.md) for workspace
selection, configuration, shortcuts, and session lifetime.

![Solmu client screenshot](../docs/screenshots/muxer.png)
