# Boxer

After [installing Boxer](../boxer/README.md#install), run `boxer -- solmu`,
`boxer -- solmu-backend`, or `boxer -- another-agent`. Running `boxer`
by itself is equivalent to `boxer -- solmu`: it opens the Solmu terminal
client. Name another program after `--` to launch it instead.
`--cwd PATH` chooses the workspace. Terminal input/output
and the program's exit status are preserved.

## Default permissions

Filesystem access and **all network requests are allowed** by default, subject
to your existing OS permissions. Linux uses Landlock (Linux 6.2+ with ABI v3),
macOS uses Seatbelt through `sandbox-exec`, and Windows uses a Job Object that
contains the process tree and terminates remaining descendants on exit.

`--read-only` denies new filesystem writes on Linux/macOS. Inherited open
handles retain their access. Windows rejects filesystem restrictions.

## Linux isolation

Install Bubblewrap (`sudo apt install bubblewrap` on Debian/Ubuntu). Run with
a delegated cgroup v2 hierarchy. On systemd 254+:

```sh
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  boxer --isolated --cwd /path/to/project -- solmu

# Customize limits; all descendants share the same budget.
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  boxer --isolated --cpus 1 --memory-mib 512 --pids 64 --cwd /path/to/project -- solmu-backend
```

This mode provides a private process tree, mount view, user namespace, hostname,
and IPC and cgroup namespaces. It drops kernel capabilities, prevents gaining new privileges
and creating more user namespaces, and provides private temporary files.
System executables, libraries, certificates, and DNS settings are read only.
Your chosen workspace is shared writable; `--read-only` makes it read only.
The executable is shared read only, including when installed in your home.

Your home, SSH agent, host process list, and host runtime sockets are not exposed.
Provider credentials (`*_API_KEY`, `*_AUTH_TOKEN`), `LLM_*`, `SOLMU_*`, and basic
terminal settings are forwarded. A `.env` inside the workspace is available.
Provider project/region settings, GitHub Models tokens, and network proxy
settings are forwarded too.
Choose a project directory, not your entire home: the selected workspace is
accessible to the program.
Kernel control directories (`/sys`, `/proc`, `/dev`) and the delegated cgroup
hierarchy cannot be selected as the workspace.

Networking remains unrestricted, including network access to host services.
The sandbox shares the host kernel; it is not a VM. The default permissive
mode does not protect your files or credentials and applies no resource limits.

### Resource limits and system calls

Isolated mode limits the entire process tree to **2 CPU cores, 2048 MiB of
memory, no swap, and 256 processes/threads** by default. Use `--cpus N`,
`--memory-mib N`, and `--pids N` to set positive integer limits. CPU use is
throttled; exceeding memory can terminate the sandbox; reaching the task limit
prevents new processes or threads. Limits include Bubblewrap's helper processes.
Remaining descendants are terminated and the per-run cgroup is removed on exit,
including Ctrl+C, termination signals, and hangups. Forced termination such as
SIGKILL cannot run cleanup; a service manager should own the delegated hierarchy.

Seccomp rejects namespace creation, mount changes, tracing, kernel modules,
kernel keyrings, BPF, io_uring, and other privileged kernel operations. Ordinary
processes, threads, file operations within the workspace, and network requests
remain available. Linux x86_64 and aarch64 are supported.

Systemd delegation is detected automatically. Your user service manager must
have the `cpu`, `memory`, and `pids` controllers delegated by the host. If it
does not, an administrator can run the same command as a system service using
`sudo systemd-run --pty --same-dir -p User="$(id -un)" -p Delegate=yes
-p DelegateSubgroup=supervisor ...`.

For an existing delegated hierarchy, pass `--cgroup-root PATH` or set
`SOLMU_CGROUP_ROOT`. It must be a real, writable cgroup v2 parent with all three
controllers available, no processes directly in it, and the launcher already
running in a child cgroup. Solmu manages only its own per-run children. It does
not move other host processes or change unrelated cgroups.

Linux isolation requires enabled unprivileged user namespaces and a Bubblewrap
version supporting `--disable-userns`, and cgroup v2 with `cgroup.kill`
(Linux 5.14+). Startup fails if controls cannot be
applied; there is no unprotected fallback. `--isolated` is Linux only.
