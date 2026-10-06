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

## Explain a path decision

Use `boxer why` to inspect how a policy treats a path without launching an
agent. It accepts the same policy options as a normal run. Relative paths are
resolved from the selected workspace; read is the default operation.

```sh
boxer why --path ~/.ssh/id_ed25519 --op read --profile codex --cwd /path/to/project
boxer why --path src/new-file.rs --op write --workspace --cwd /path/to/project
```

The JSON result says whether the resolved policy allows or denies the access,
or reports `unsupported` when the current platform backend cannot enforce the
requested filesystem policy. This explains Boxer’s policy rules, not ambient
OS permissions, ACLs, or whether a later filesystem operation will succeed.

## Verify trusted instruction files

Boxer can verify signed instruction files before starting an agent. Create a
keypair once and keep the private key private:

```sh
boxer trust keygen --private-key ~/.boxer/keys/instructions.pk8 --public-key trusted-instructions.pub
boxer trust sign --key ~/.boxer/keys/instructions.pk8 AGENTS.md
boxer trust verify --key trusted-instructions.pub AGENTS.md
```

Signing creates an `AGENTS.md.boxer.sig` sidecar. Share the public key through a
trusted channel, then require verification on launch. Remove the old signature
sidecar before signing an updated file again:

```sh
boxer --trust-key trusted-instructions.pub --verify AGENTS.md \
  --profile solmu --cwd /path/to/project -- solmu
```

For repeatable checks, create a trust policy listing workspace-relative files
and sign the policy itself as well as each listed file:

```json
{"version":1,"files":["AGENTS.md",".claude/CLAUDE.md"]}
```

```sh
boxer trust sign --key ~/.boxer/keys/instructions.pk8 AGENTS.md
boxer trust sign --key ~/.boxer/keys/instructions.pk8 .claude/CLAUDE.md
boxer trust sign --key ~/.boxer/keys/instructions.pk8 boxer-trust.json
boxer --trust-key trusted-instructions.pub --trust-policy boxer-trust.json \
  --profile solmu --cwd /path/to/project -- solmu
```

Boxer verifies the policy signature first, then requires valid signatures for
every listed file before launching the agent. The policy and listed files must
be regular files inside the selected workspace. Relative paths are resolved
from the workspace. Alternatively, repeat `--verify FILE` for explicit per-run
file checks. The private key is created with owner-only permissions on Unix.
Protect and back it up securely; losing it means you cannot sign future updates.

This is explicit Ed25519 file-signature verification anchored to the public key
you provide. Boxer does not yet provide automatic trust-policy discovery,
publisher identity or revocation rules, or Sigstore/CI provenance verification.
The signature is checked before process launch; it does not make the file
immutable against later changes by other processes.

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
symbolic links inside the project do not grant access to files outside it.

The profile clears unrelated environment variables and sets `SOLMU_WORKSPACE`
to the selected project. Provider keys, `LLM_*`, `SOLMU_*`, terminal settings,
and proxy settings remain available. `SSH_AUTH_SOCK` and unrelated secrets are
not forwarded. For credentials that should not remain in your shell environment,
Boxer can load selected values from the operating system credential store.

### Store credentials for an agent

Save a credential without putting its value in shell history or command-line
arguments. Boxer prompts for it without echoing the input:

```sh
boxer credential set OPENAI_API_KEY
boxer credential status OPENAI_API_KEY
boxer --profile solmu --env-credential OPENAI_API_KEY --cwd /path/to/project -- solmu
```

Use `boxer credential delete OPENAI_API_KEY` to remove it. The credential name
is also the environment variable provided to the launched program. Multiple
`--env-credential NAME` options are supported, and policies can list names in
`env_credentials`. Boxer stores values in the OS credential store (Keychain on
macOS, Credential Manager on Windows, and Secret Service on Linux). On Linux,
a Secret Service provider must be available to the user session.

This makes the value easier to manage and avoids exposing it in Boxer command
arguments. The value is still present in the agent's environment and available
to that agent and its child processes.

For Linux launches, use `--credential` to keep the real provider key outside
the agent process. Boxer sends provider requests through a local proxy, gives
the agent a random session token, and adds the real key to the upstream HTTPS
request:

```sh
boxer credential set OPENAI_API_KEY
boxer --profile solmu --isolated --network proxy --credential openai \
  --cwd /path/to/project -- solmu
```

The matching provider host is allowed automatically. Use
`--credential anthropic` with a stored `ANTHROPIC_API_KEY` for Anthropic, or
`--credential gemini` with a stored `GEMINI_API_KEY` for Gemini. Gemini requests
are routed to `generativelanguage.googleapis.com` and the broker adds the key as
`x-goog-api-key`. This
requires Linux isolated mode with routed networking; other network destinations
still need explicit routes. The local broker verifies the session token and
does not print the stored key. The agent can make requests through the broker,
but the key itself is not in its environment.

By default, the broker allows any API path on the selected provider. Add one or
more `--allow-endpoint PROVIDER:METHOD:PATH` options to limit it to specific
endpoints. Once rules are present, other requests receive `403 Forbidden`:

```sh
boxer --profile solmu --isolated --network proxy --credential openai \
  --allow-endpoint openai:POST:/v1/chat/completions \
  --allow-endpoint openai:POST:/v1/responses \
  --cwd /path/to/project -- solmu
```

Methods are uppercase HTTP method names or `*`. In paths, `*` matches one
non-empty segment and `**` matches zero or more segments. Query strings are not
part of the match. The same rules can be stored as `endpoint_rules` entries
with `provider`, `method`, and `path` fields in a Boxer policy file.

### Give access to an installed toolchain

Runtime groups grant read-only access to detected Node, Python, Rust, or Go
installations and caches while keeping the rest of your home directory outside
the workspace policy:

```sh
boxer --profile solmu --runtime-group rust --cwd /path/to/project -- solmu
boxer --workspace --runtime-group node --runtime-group python \
  --cwd /path/to/project -- another-agent
```

Groups can also be listed in a policy's `runtime_groups` array. Boxer checks
standard install locations and the matching variables such as `CARGO_HOME`,
`RUSTUP_HOME`, `NVM_DIR`, and `GOPATH`. Missing locations are skipped. The Rust
group grants Cargo's `bin`, `registry`, and `git` directories plus the Rust
toolchain directory; it does not grant the Cargo home root. Runtime groups are
opt-in and require `--workspace` or `--isolated`. They grant read access, so
package managers that need to update caches may also need a writable cache
directory granted with `--write`.

### Run Pi, OpenCode, Claude Code, or Codex

Install the agent's CLI first, then start it in a project on Linux or macOS:

```sh
boxer --profile codex --cwd /path/to/project
boxer --profile claude-code --cwd /path/to/project
boxer --profile opencode --cwd /path/to/project
boxer --profile pi --cwd /path/to/project
```

These profiles start `opencode`, `claude`, `codex`, and `pi`. They allow writes to
the project and keep each agent's login and settings in its own directory under
`~/.boxer/profiles/`. OpenCode's config, data, cache, logs, and state each use a
private directory there. Pi uses its own private agent directory there. Sign in
the first time you run a profile. A login in your usual CLI home is separate
from its Boxer login. The profiles forward provider credentials and settings
needed by each agent, along with terminal and
proxy settings. They leave unrelated environment variables out. All network
requests remain allowed by default.

Install the agent CLI before using its profile. OpenCode uses the `opencode`
command; Pi uses the `pi` command. See the [OpenCode installation guide](https://opencode.ai/docs/)
and [Pi coding agent guide](https://github.com/badlogic/pi-mono/tree/main/packages/coding-agent).

## Review and restore a session

Add `--rollback` to save the workspace before and after an agent runs:

```sh
boxer --rollback --profile solmu --cwd /path/to/project -- solmu
boxer rollback list
boxer rollback show <session-id> --diff
boxer rollback restore <session-id> --dry-run
boxer rollback restore <session-id>
```

The session list shows the command, workspace, and number of changed paths. The
diff lists added, changed, and deleted paths. Restore replaces the workspace
with its saved pre-session contents, including file contents, supported file
permissions, and symbolic links. Review the preview before restoring; changes
made after that session will also be discarded. Boxer stores snapshots in
`.boxer/rollback` under your home folder by default and deduplicates identical
file contents. Set `BOXER_ROLLBACK_DIR` to store them elsewhere. Snapshot storage
must be outside the workspace. If a session ends before its final snapshot is
saved, you can still restore its pre-session snapshot. The preview lists changes
from the recorded session; it does not include edits made after that session.

Rollback covers files inside the workspace. It does not undo network requests or
changes made elsewhere on the machine. Snapshots are local and can use substantial
disk space for large workspaces.

Prune old session records and unreferenced snapshot data. Preview first:

```sh
boxer rollback cleanup --older-than 30 --dry-run
boxer rollback cleanup --keep 10 --dry-run
boxer rollback cleanup --older-than 30 --keep 10
```

When both limits are set, sessions older than the age limit or beyond the
newest-session limit are removed. Shared content objects remain while any
retained session references them. Cleanup permanently removes selected session
records, including their audit records.

### Audit a session

Each rollback session records a start and completion event and a SHA-256 digest
for each changed path's snapshot entry. Boxer chains the records with a local
HMAC key stored beside the session data. Inspect or verify them with:

```sh
boxer rollback audit list
boxer rollback audit show <session-id>
boxer rollback audit verify <session-id>
```

This verifies the integrity of Boxer’s local lifecycle and snapshot records. It
does not record every system call, denied access, or network request, and it is
not remote or hardware-backed evidence. Anyone who can modify both the audit
store and its local key can replace the records. Treat it as a local tamper
check, not a complete security audit.

If your CLI or its dependencies are installed outside the system runtime and
project, grant the installation directory with `--read PATH`. Use `--write PATH`
for any additional folder the agent needs to change. `boxer --profile codex
--cwd /path/to/project --print-policy` shows the grants before launch. You can
also put a different program after `--` while keeping the profile's permissions.
The workspace profiles require Linux or macOS; native Windows currently rejects
filesystem restrictions.

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

Networking remains unrestricted by default, including local services. This mode restricts
filesystem operations; it does not create private processes or resource limits.
macOS also permits file metadata lookups, listing the root directory for system
library startup, terminal I/O, and system IPC used by native services. The root
directory grant is not recursive and does not allow reading files below it.
Already open handles retain their access. Files inside the project, including
`.env` and `.git`, remain accessible under the project's grant.

### Protect the backend's tools

Solmu's tools execute in the **backend** process. Sandboxing a client does not
sandbox an already running backend. To apply kernel restrictions to Bash and
other tool calls, launch a separate backend inside Boxer:

```sh
boxer --profile solmu --cwd /path/to/project -- solmu-backend

# In another terminal, connect a client to that backend.
solmu
```

The Solmu profile sets the default thread workspace to that project. The default
SQLite database lives there too. If your backend uses a database or published
web files elsewhere, grant their directory explicitly with `--write` or `--read`
and keep the corresponding `SOLMU_*` settings. File operations outside the
declared paths fail, including Bash commands reaching outside the project.
Stop your regular backend before starting its boxed replacement on the default
port, 3000. See [service management](services.md).

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

### Check whether a policy can run

Use `--check` to apply the resolved permissions to a short-lived Boxer probe:

```sh
boxer --profile solmu --cwd /path/to/project --check
boxer --workspace --network deny --cwd /path/to/project --check
```

Success returns JSON with `"enforcement": "checked"` and exits with status 0.
Failure exits with status 125 and explains the unavailable control. Your agent
is never started, even if you provide a program after `--`. The check does not
verify that program's installation, libraries, configuration, or API credentials.
It checks the current OS permissions, not future availability at launch time.
Choose either `--check` or `--print-policy`: a preview only describes permissions,
while a check starts a probe to apply them.

For Linux isolated policies, run the check in the same delegated cgroup as the
agent. It tests Bubblewrap, namespaces, seccomp, and the resource limits, then
removes the probe's cgroup:

```sh
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  boxer --isolated --network deny --cwd /path/to/project --check
```

On Windows, the permissive check verifies Job Object containment. Checks for
unsupported filesystem, network, or isolated policies fail.

### Policy paths and fields

Relative paths in a policy are resolved against its containing directory.
`$HOME` and `$WORKSPACE` can prefix a path using `/`; other variables, shell
commands, and wildcards are not expanded. Command-line grants extend the policy;
mode and resource options override file values, and `--read-only` and
`--clean-env` can tighten them. Choose either `--policy` or `--profile`.

The required fields are `version: 1` and `mode`, which is `unrestricted`,
`workspace`, or `isolated`. Optional fields are `read_only`, `read`, `write`,
`network` (`allow`, `deny`, or `proxy`), `network_profile`, `hosts`, `deny_hosts`,
`local`, `publish`, `clean_env`, `upstream_proxy`, `upstream_bypass`, `pass_env`, `env_credentials`,
`credentials`, `runtime_groups`, `cpus`, `endpoint_rules`, `memory_mib`, `pids`,
and `cgroup_root`. Endpoint rules have
`provider`, `method`, and `path` fields and require a matching entry in
`credentials`. Proxy credentials are `openai`, `anthropic`, and `gemini`; runtime groups
are `node`, `python`, `rust`, and `go`.
Unknown or duplicate fields, invalid values, missing grant paths, and files over 1 MB are
rejected before launch. Resource controls require `isolated` mode.

Inspect saved policy files without launching the agent:

```sh
boxer policy init
boxer policy init --output ./agent-policy.json
boxer policy schema
boxer policy profiles
boxer policy validate ./boxer-policy.json --cwd /path/to/project
boxer policy show ./boxer-policy.json --cwd /path/to/project
boxer policy diff ./before.json ./after.json --cwd /path/to/project
```

`policy init` creates a workspace policy that allows network access and starts
with no additional path grants. It refuses to overwrite an existing file. Edit
the generated JSON, then validate it before using `--policy` to launch a program.
`policy schema` prints the JSON Schema for editors and other JSON tooling;
`policy validate` also checks combinations that depend on Boxer runtime rules.

`validate` parses and resolves the policy and reports whether Boxer has a
backend for it on the current platform. `show` prints its resolved values, and
`diff` shows changed fields. Validation does not apply the kernel controls; use
`boxer --policy FILE --cwd PATH --check` to test enforcement in a probe process.
The profiles command lists Boxer’s built-in agent launch profiles.

## Run an offline command

Linux and macOS can deny socket networking explicitly:

```sh
boxer --workspace --network deny --cwd /path/to/project -- /bin/sh ./script.sh
```

The restriction applies to threads and subprocesses. It blocks IPv4 and IPv6,
TCP listening and connections, UDP, and connections to Unix sockets. Localhost
is blocked too; proxy environment variables do not grant an exception.
Use `--network allow` for the default unrestricted socket networking. The same
choice can be saved as `"network": "deny"` in a policy file, with a command-line
value overriding that file.

Offline launches close inherited descriptors above standard input/output/error
on exec and reject socket-based standard I/O before starting the program.
Normal terminal and pipe I/O remain available. Other explicit communication
channels, such as shared writable files, caller-provided pipes, and macOS Mach
services, retain their permissions. Combine network denial with workspace or
isolated permissions to limit filesystem access too.

On Linux, seccomp also blocks io_uring, tracing, copying another process's
descriptors, namespace changes, and privileged kernel operations. Isolated mode
adds a private network namespace. macOS uses Seatbelt network rules. Failure to
apply a required control stops launch; native Windows currently rejects the
offline policy before starting a process.

Solmu's clients and backend need network access to communicate and call an LLM.
Keep networking allowed for those processes; use the offline option for
standalone commands that do not need their backend or a provider connection.

## Choose allowed network destinations on Linux

Use `--isolated --network proxy` to give a sandbox a private network with explicit
routes. It requires the same Bubblewrap and delegated cgroup setup as Linux
isolation below. There is no direct route to the Internet or the host network.
HTTP requests and HTTPS CONNECT tunnels go through Boxer's local proxy;
overriding proxy variables or disabling a program's proxy does not restore
direct access. Connections to pathname and abstract Unix sockets, raw network
families, tracing, and copying another process's descriptors are denied. Broker channels and host-connected
sockets are not inherited by the launched agent.

Built-in host sets can reduce setup for common workflows:

```sh
boxer network profiles
boxer --isolated --network proxy --network-profile minimal \
  --cwd /path/to/project -- solmu
boxer --isolated --network proxy --network-profile developer \
  --allow-host packages.example.com --cwd /path/to/project -- solmu
```

`minimal` allows OpenAI, Anthropic, and Google Generative Language API hosts.
`developer` adds GitHub, npm/Python/Rust registries, Sigstore, and selected
language documentation hosts. `claude-code`, `codex`, and `opencode` are named
presets with the same network destinations as `developer`; they do not launch
the corresponding client or configure credentials. `enterprise` adds wildcard
routes for Google APIs, Azure AI, and Amazon Bedrock. Use the separate
`--profile` option for Boxer’s client launch profiles. You can add exact hosts
or hostname patterns with `--allow-host`; policy-file `hosts` are also combined
with the selected network profile. These are host-level grants, not endpoint
filters.
Package registries may use additional CDN hosts, which you can add explicitly.
Network profiles configure allowed hostnames, not credentials; use
`--credential` separately when the agent should receive a proxy session token
instead of the real provider key.

`--allow-host DOMAIN` also accepts complete-label wildcards such as
`*.example.com` (one or more subdomain labels) and
`build.*.ci.example.com` (exactly one label). The requested port must match the
grant. Use `--deny-host DOMAIN` or policy `deny_hosts` to block a hostname even when
an allowlist or network profile includes it. Deny patterns are checked before
connections and credential proxy startup. Patterns accept an exact hostname,
`*` for every hostname, `*.example.com` for one or more subdomain labels, or a
complete `*` label such as `build.*.ci.example.com` for exactly one label.
Metadata service hostnames are always denied. Routed connections also reject
private, loopback, link-local, and other special-use IP destinations.

To send routed connections through a corporate HTTP CONNECT proxy, pass
`--upstream-proxy` or set `BOXER_UPSTREAM_PROXY`:

```sh
boxer --isolated --network proxy --network-profile developer \
  --upstream-proxy http://proxy.corp.example:3128 \
  --cwd /path/to/project -- solmu
```

Proxy credentials can be included in the URL using HTTP Basic authentication;
use the environment variable rather than a command-line argument for a URL
that contains a password. Boxer redacts proxy credentials from resolved policy
output. Policy files can set `upstream_proxy` too.

Use `--upstream-bypass DOMAIN` to connect directly for an exact domain or
`*.DOMAIN` pattern. `BOXER_UPSTREAM_BYPASS` accepts a comma-separated list of
the same patterns. A bypass does not add a network route: the target must also
match `--allow-host`, a network profile, or policy `hosts`. Explicit bypasses
may reach public or private network addresses, but loopback, link-local, and
other special-use addresses remain blocked. These settings also apply to
brokered credential traffic.

For a backend that calls OpenAI, stop the regular backend, then run:

```sh
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  boxer --profile solmu --isolated --network proxy \
  --allow-host api.openai.com --publish 3000 \
  --cwd /path/to/project -- solmu-backend

# In another terminal, use the usual client and default backend port.
solmu
```

`--allow-host HOST[:PORT]` allows one exact DNS hostname and port, defaulting to
443. Repeat it for other services, for example `--allow-host api.anthropic.com`.
Wildcards, URLs, embedded credentials, and numeric IP addresses are rejected.
The broker resolves names outside the sandbox and rejects private, loopback,
and special-use addresses. TLS stays between the client and the destination;
Boxer controls the destination, not encrypted API paths, HTTP methods, or bodies.
TCP tunnels can carry any protocol supported by an allowed destination.

`--publish PORT` exposes a guest service on the same **host loopback** port.
For Solmu's backend, `--publish 3000` keeps HTTP, streaming replies, and WebSocket
updates available to all clients using their normal settings. Publishing does
not grant a route from the guest to other host services.

To run the CLI inside a private network while its backend runs outside it:

```sh
systemd-run --user --pty --same-dir -p Delegate=yes -p DelegateSubgroup=supervisor \
  boxer --isolated --network proxy --allow-local 127.0.0.1:3000 \
  --cwd /path/to/project -- solmu
```

`--allow-local IP:PORT` forwards exactly that host loopback service into the
guest at the same address and port. IPv6 uses `[::1]:PORT`. This supports direct
TCP and WebSocket clients as well as HTTP clients. Other host loopback ports
remain inaccessible. Do not publish and forward the same guest port.
Proxy and `NO_PROXY` settings are set automatically for these routes.

Routes can be saved in an explicit policy:

```json
{
  "version": 1,
  "mode": "isolated",
  "network": "proxy",
  "hosts": ["api.openai.com:443"],
  "local": [],
  "publish": [3000]
}
```

Use `--check` in your delegated cgroup to verify this policy before launching.
It also checks that published ports are available. Up to 64 active connections
are handled per sandbox; idle connections close after five minutes. Socket
creation restrictions can affect programs that require named Unix sockets.
Anonymous Unix stream pairs remain available for local runtime IPC.
macOS and Windows currently reject routed policies before launching a program.
Permissive networking remains the default on every platform.

Ordinary network routes do not hide provider keys: environment variables you
forward still reach the agent, and `.env` files within shared paths remain
readable. Use `--credential openai`, `--credential anthropic`, or
`--credential gemini` on Linux for
provider-key proxy injection. This does not protect other secrets stored in the
workspace.

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

Networking remains unrestricted by default, including network access to host services.
Add `--network deny` to run an offline command in a private network namespace.
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
processes, threads, and file operations within the workspace remain available.
Network requests remain unrestricted with `--network allow`; `deny` blocks them,
and `proxy` permits only declared routes.
Linux x86_64 and aarch64 are supported.

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
