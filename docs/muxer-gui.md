# Muxer GUI

Muxer GUI is a standalone native Rust desktop app with the Muxer workspace
engine embedded. It shares Muxer's session files, so spaces and tabs are
available in both apps. Do not open the same session in both apps at once.

Start Muxer GUI from a project folder:

```sh
muxer-gui
```

It opens the local `default` session, or creates one in the current folder.
Session state is stored in the same local directory as Muxer. No `muxer`
binary, daemon, or local control server is needed. The GUI installer includes
the `solmu` runtime for Solmu spaces; start the Solmu backend before using
those spaces. Terminal spaces launch local shells and command-line clients.

The sidebar shows the session, its spaces, tabs, and panes. Create spaces and
tabs, split panes, send terminal input, and switch spaces. Choose **Solmu** or
**Terminal** from the dropdown beside **Spaces** when creating a space.
Solmu opens the shared native Solmu desktop interface, including its
conversation list and Profile, Audit, Tasks, and other pages. Terminal keeps
the terminal view for shells and command-line clients such as Claude Code.

Muxer GUI owns the session lock while open. Close it before opening that
session in the terminal Muxer app; the session layout and metadata are saved
to the shared local state.

See [installation](releases.md#individual-components), [Muxer usage](muxer.md),
and [development](development.md).

![Muxer GUI](screenshots/muxer-gui.png)
