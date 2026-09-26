# Solmu sandbox

A Rust launcher for Solmu and other programs using native kernel controls.
Filesystem access and **all network requests are allowed by default**.

```sh
cargo build -p solmu-sandbox
sandbox -- solmu-cli
sandbox --cwd /path/to/project -- another-agent
```

On Linux, choose container-style isolation with a private process tree and
filesystem view, a writable workspace, and read-only system files:

```sh
sudo apt install bubblewrap
sandbox --isolated --cwd /path/to/project -- solmu-cli
```

Networking stays allowed. This uses the host kernel. Without a program, the
launcher starts `solmu-cli`. Use `--read-only` on Linux/macOS to deny filesystem
writes; Windows supports process-tree containment only.

See [permissions, platform requirements, and usage](../docs/sandbox.md).
