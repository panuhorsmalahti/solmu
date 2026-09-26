# Sandbox

Build with `cargo build -p solmu-sandbox`. Run `sandbox -- solmu-cli`,
`sandbox -- solmu-backend`, or `sandbox -- another-agent`. Without a program,
it starts `solmu-cli`. `--cwd PATH` chooses the workspace. Terminal input/output
and the program's exit status are preserved.

## Default permissions

Filesystem access and **all network requests are allowed** by default, subject
to your existing OS permissions. Linux uses Landlock (Linux 6.2+ with ABI v3),
macOS uses Seatbelt through `sandbox-exec`, and Windows uses a Job Object that
contains the process tree and terminates remaining descendants on exit.

`--read-only` denies new filesystem writes on Linux/macOS. Inherited open
handles retain their access. Windows rejects filesystem restrictions.

## Linux isolation

Install Bubblewrap (`sudo apt install bubblewrap` on Debian/Ubuntu), then:

```sh
sandbox --isolated --cwd /path/to/project -- solmu-cli
sandbox --isolated --cwd /path/to/project -- solmu-backend
sandbox --isolated --read-only --cwd /path/to/project -- another-agent
```

This mode provides a private process tree, mount view, user namespace, hostname,
and IPC namespace. It drops kernel capabilities, prevents gaining new privileges
and creating more user namespaces, and provides private temporary files.
System executables, libraries, certificates, and DNS settings are read only.
Your chosen workspace is shared writable; `--read-only` makes it read only.
The executable is shared read only, including when installed in your home.

Your home, SSH agent, host process list, and host runtime sockets are not exposed.
Provider credentials (`*_API_KEY`, `*_AUTH_TOKEN`), `LLM_*`, `SOLMU_*`, and basic
terminal settings are forwarded. A `.env` inside the workspace is available.
Choose a project directory, not your entire home: the selected workspace is
accessible to the program.

Networking remains unrestricted, including network access to host services.
The sandbox shares the host kernel; it is not a VM. CPU, memory, and process
limits and syscall filtering are not implemented yet. The default permissive
mode does not protect your files or credentials.

Linux isolation requires enabled unprivileged user namespaces and a Bubblewrap
version supporting `--disable-userns`. Startup fails if controls cannot be
applied; there is no unprotected fallback. `--isolated` is Linux only.
