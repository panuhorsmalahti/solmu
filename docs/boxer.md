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

## Limit access to a project

On Linux and macOS, use the built-in Solmu profile for a lightweight workspace
sandbox. It requires no Bubblewrap or delegated cgroup:

```sh
boxer --profile solmu --cwd /path/to/project -- solmu
```

The project is writable. System executables, libraries, DNS settings, and
certificates are readable, along with the launched executable. Executable
mapping is allowed within the readable paths so programs and libraries can load. File contents
outside these paths are denied. Restrictions are inherited by subprocesses;
links inside the project do not grant access to files outside it.

The profile clears unrelated environment variables and sets `SOLMU_WORKSPACE`
to the selected project. Provider keys, `LLM_*`, `SOLMU_*`, terminal settings,
and proxy settings remain available. `SSH_AUTH_SOCK` and unrelated secrets are
not forwarded. Provider keys are still visible to the launched program; this
is environment filtering, not credential brokering.

Use `--workspace` for the same filesystem permissions without the Solmu-specific
environment defaults. Add `--clean-env` to filter its environment, or
`--pass-env NAME` to forward an additional variable. These options work with
permissive mode too. Existing OS permissions still apply.

Add explicit access to dependencies, linked skills, or result directories:

```sh
boxer --profile solmu --cwd /path/to/project \
  --read /path/to/reference-project \
  --read /path/to/installed-skills \
  --write /path/to/results -- solmu
```

`--read` and `--write` can be repeated and require existing files or directories.
Relative command-line paths are resolved against the workspace; symbolic links
are resolved before launch. A directory grant covers its descendants, and a
file grant covers that existing file. Create result directories before granting
them. Writable access includes reading. Grants are additive: `--read` within a
writable project does not make that subtree read-only.
Tools that replace a file by renaming a temporary file need a writable directory
grant. Solmu's file tools also retain their own workspace boundaries; a Boxer
grant does not remove those application checks.

Add `--read-only` to prevent project writes. It cannot be combined with writable
grants. Terminal I/O and basic device access remain available. Workspace mode
does not grant a shared temporary directory; point `TMPDIR` at an existing
directory inside your project and add `--pass-env TMPDIR` if a tool needs
temporary files under a clean environment.

Networking remains unrestricted, including local services. This mode restricts
filesystem operations; it does not create private processes or resource limits.
macOS also permits file metadata lookups and system IPC used by native services.
Already open handles retain their access. Files inside the project, including
`.env` and `.git`, remain accessible under the project's grant.

### Protect the backend's tools

Solmu's tools execute in the **backend** process. Sandboxing a client does not
sandbox an already running backend. To apply kernel restrictions to Bash and
other tool calls, launch a separate backend inside Boxer:

```sh
SOLMU_BIND_ADDR=127.0.0.1:3001 \
  boxer --profile solmu --cwd /path/to/project -- solmu-backend

# In another terminal, connect a client to that backend.
SOLMU_BACKEND_URL=http://127.0.0.1:3001 solmu
```

The Solmu profile sets the default thread workspace to that project. The default
SQLite database lives there too. If your backend uses a database or published
web files elsewhere, grant their directory explicitly with `--write` or `--read`
and keep the corresponding `SOLMU_*` settings. File operations outside the
declared paths fail, including Bash commands reaching outside the project.

Native Windows currently contains process trees with a Job Object; it rejects
workspace policies rather than launching without filesystem protection.

## Reuse and inspect policies

Policy files are explicit, versioned JSON. Boxer never loads a policy merely
because it is present in a project. Save a file such as `boxer.json`:

```json
{
  "version": 1,
  "mode": "workspace",
  "clean_env": true,
  "read": ["../reference-project", "$HOME/.cargo"],
  "write": ["$WORKSPACE/results"],
  "pass_env": ["CARGO_HOME"]
}
```

```sh
boxer --policy /path/to/boxer.json --cwd /path/to/project -- solmu
boxer --policy /path/to/boxer.json --cwd /path/to/project --print-policy -- solmu
```

`--print-policy` prints the resolved policy, runtime paths, program arguments,
and forwarded environment **names** as JSON. It does not start a program,
apply restrictions, or print environment values. `platform_supported` describes
OS support, not a kernel or resource-delegation readiness check. Starting a
program still fails if required kernel controls cannot be applied.

Relative paths in a policy are resolved against its containing directory.
`$HOME` and `$WORKSPACE` can prefix a path using `/`; other variables, shell
commands, and wildcards are not expanded. Command-line grants extend the policy;
mode and resource options override file values, and `--read-only` and
`--clean-env` can tighten them. Choose either `--policy` or `--profile`.

The required fields are `version: 1` and `mode`, which is `unrestricted`,
`workspace`, or `isolated`. Optional fields are `read_only`, `read`, `write`,
`clean_env`, `pass_env`, `cpus`, `memory_mib`, `pids`, and `cgroup_root`.
Unknown or duplicate fields, invalid values, missing grant paths, and files over 1 MB are
rejected before launch. Resource controls require `isolated` mode.

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
`--read` and `--write` also expose explicitly granted existing paths in this mode.
The executable is shared read only, including when installed in your home.

Your home, SSH agent, host process list, and host runtime sockets are not exposed.
Explicit grants can expose those paths again; only grant the directories a task
needs. Granting a parent of a private temporary mount also replaces that mount
with the explicitly shared path.
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
