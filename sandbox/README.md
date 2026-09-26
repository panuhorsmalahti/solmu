# Solmu sandbox

A Rust launcher for Solmu and other programs using native kernel controls.
Filesystem access and **all network requests are allowed by default**.

```sh
cargo build -p solmu-sandbox
sandbox -- solmu-cli
sandbox --cwd /path/to/project -- another-agent
```

On Linux, choose container-style isolation with a private process tree and
filesystem view, a writable workspace, read-only system files, filtered system
calls, and CPU/memory/task limits:

```sh
sudo apt install bubblewrap
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  sandbox --isolated --cwd /path/to/project -- solmu-cli
```

Networking stays allowed. This uses the host kernel. Without a program, the
launcher starts `solmu-cli`. Use `--read-only` on Linux/macOS to deny filesystem
writes; Windows supports process-tree containment only.

See [permissions, platform requirements, and usage](../docs/sandbox.md).
