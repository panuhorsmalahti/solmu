use crate::policy::{self, Mode, Network as NetworkPolicy, Policy};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Trace {
    read: BTreeSet<PathBuf>,
    write: BTreeSet<PathBuf>,
    outbound: BTreeMap<(String, u16), usize>,
    listening: BTreeMap<(String, u16), usize>,
}

#[derive(Serialize)]
struct Report {
    filesystem: Filesystem,
    network: Network,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_gaps: Option<PolicyGaps>,
    command_exit_code: Option<i32>,
    timed_out: bool,
}

#[derive(Serialize)]
struct Filesystem {
    read: Vec<PathBuf>,
    write: Vec<PathBuf>,
    read_write: Vec<PathBuf>,
}

#[derive(Serialize)]
struct Network {
    outbound: Vec<Endpoint>,
    listening: Vec<Endpoint>,
}

#[derive(Serialize)]
struct Endpoint {
    address: String,
    port: u16,
    count: usize,
}

#[derive(Serialize)]
struct PolicyGaps {
    filesystem: Filesystem,
    network: NetworkGaps,
}

#[derive(Serialize)]
struct NetworkGaps {
    outbound_denied: Vec<Endpoint>,
    outbound_hostname_check_needed: Vec<Endpoint>,
    listening_denied: Vec<Endpoint>,
    listening_not_published: Vec<Endpoint>,
}

pub fn command(arguments: &[OsString]) -> io::Result<i32> {
    let mut json = false;
    let mut timeout = None;
    let mut policy_file = None;
    let mut index = 1;
    while index < arguments.len() && arguments[index] != "--" {
        if arguments[index] == "--json" {
            json = true;
            index += 1;
        } else if arguments[index] == "--timeout" && index + 1 < arguments.len() {
            let seconds: u64 = arguments[index + 1]
                .to_str()
                .and_then(|value| value.parse().ok())
                .filter(|seconds| *seconds > 0 && *seconds <= 86_400)
                .ok_or_else(|| io::Error::other("--timeout requires seconds from 1 to 86400"))?;
            timeout = Some(Duration::from_secs(seconds));
            index += 2;
        } else if arguments[index] == "--policy" && index + 1 < arguments.len() {
            if policy_file.is_some() {
                return Err(io::Error::other("Specify only one --policy"));
            }
            policy_file = Some(PathBuf::from(&arguments[index + 1]));
            index += 2;
        } else {
            return Err(usage());
        }
    }
    if arguments.get(index).is_none_or(|argument| argument != "--")
        || arguments.get(index + 1).is_none()
    {
        return Err(usage());
    }
    let program = &arguments[index + 1];
    let command_arguments = &arguments[index + 2..];
    let directory = std::env::current_dir()?;
    let comparison_policy = policy_file
        .map(|file| {
            let mut policy = Policy::from_file(&file)?;
            policy.resolve(&directory)?;
            Ok::<_, io::Error>(policy)
        })
        .transpose()?;
    let executable = if comparison_policy.is_some() {
        let mut command = Command::new(program);
        command.current_dir(&directory);
        policy::executable(&command).ok()
    } else {
        None
    };
    let temporary = tempfile::tempdir()?;
    let trace_file = temporary.path().join("syscalls.log");
    let mut command = Command::new("strace");
    command
        .args([
            OsString::from("-f"),
            OsString::from("-qq"),
            OsString::from("-y"),
            OsString::from("-s"),
            OsString::from("8192"),
            OsString::from("-e"),
            OsString::from("trace=%file,%network"),
            OsString::from("-o"),
            trace_file.as_os_str().to_owned(),
            OsString::from("--"),
            program.clone(),
        ])
        .args(command_arguments)
        .current_dir(&directory)
        .process_group(0);
    let child_stdout = temporary.path().join("stdout.log");
    let child_stderr = temporary.path().join("stderr.log");
    if json {
        command
            .stdout(Stdio::from(std::fs::File::create(&child_stdout)?))
            .stderr(Stdio::from(std::fs::File::create(&child_stderr)?));
    }
    let child = command.spawn().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            io::Error::other(
                "Boxer learn requires strace; install it with your Linux package manager",
            )
        } else {
            error
        }
    })?;
    let (status, timed_out) = wait(child, timeout)?;
    let trace = parse(&std::fs::read_to_string(&trace_file)?, &directory);
    let policy_gaps = comparison_policy
        .as_ref()
        .map(|policy| trace.policy_gaps(policy, &directory, executable.as_deref()));
    let report = trace.report(status.code(), timed_out, policy_gaps);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(io::Error::other)?
        );
        copy_to_stderr(&child_stdout)?;
        copy_to_stderr(&child_stderr)?;
    } else {
        print_report(&report);
    }
    Ok(if timed_out {
        124
    } else {
        status.code().unwrap_or(1)
    })
}

fn copy_to_stderr(path: &Path) -> io::Result<()> {
    let mut file = std::fs::File::open(path)?;
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    io::copy(&mut file, &mut stderr)?;
    Ok(())
}

fn wait(
    mut child: std::process::Child,
    timeout: Option<Duration>,
) -> io::Result<(ExitStatus, bool)> {
    let Some(timeout) = timeout else {
        return child.wait().map(|status| (status, false));
    };
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status, false));
        }
        if start.elapsed() >= timeout {
            // Boxer starts strace in a new process group so the tracee and its
            // ordinary descendants are stopped together when the limit hits.
            // SAFETY: a negative PID targets the process group created above.
            if unsafe { libc::kill(-(child.id() as i32), libc::SIGTERM) } != 0 {
                child.kill()?;
            }
            std::thread::sleep(Duration::from_millis(250));
            if child.try_wait()?.is_none() {
                // SAFETY: this targets only the process group created above.
                if unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) } != 0 {
                    child.kill()?;
                }
            }
            return child.wait().map(|status| (status, true));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

impl Trace {
    fn report(
        self,
        command_exit_code: Option<i32>,
        timed_out: bool,
        policy_gaps: Option<PolicyGaps>,
    ) -> Report {
        let read_write: Vec<_> = self.read.intersection(&self.write).cloned().collect();
        let read: Vec<_> = self.read.difference(&self.write).cloned().collect();
        let write: Vec<_> = self.write.difference(&self.read).cloned().collect();
        Report {
            filesystem: Filesystem {
                read,
                write,
                read_write,
            },
            network: Network {
                outbound: self.outbound.into_iter().map(endpoint_record).collect(),
                listening: self.listening.into_iter().map(endpoint_record).collect(),
            },
            policy_gaps,
            command_exit_code,
            timed_out,
        }
    }

    fn policy_gaps(
        &self,
        policy: &Policy,
        workspace: &Path,
        executable: Option<&Path>,
    ) -> PolicyGaps {
        let mut readable = policy::runtime_paths();
        readable.extend(policy.read.iter().cloned());
        readable.extend(policy.write.iter().cloned());
        if policy.mode != Mode::Unrestricted {
            readable.push(workspace.to_owned());
            readable.extend(policy::device_paths());
        }
        if let Some(executable) = executable {
            readable.push(executable.to_owned());
        }

        let mut writable = policy.write.clone();
        writable.extend(policy.write_only.iter().cloned());
        if policy.mode != Mode::Unrestricted {
            writable.extend(policy::device_paths());
        }
        if !policy.read_only && policy.mode != Mode::Unrestricted {
            writable.push(workspace.to_owned());
        }

        let read_write_observed: BTreeSet<_> =
            self.read.intersection(&self.write).cloned().collect();
        let read_observed: BTreeSet<_> = self.read.difference(&self.write).cloned().collect();
        let write_observed: BTreeSet<_> = self.write.difference(&self.read).cloned().collect();
        let read_write = read_write_observed
            .iter()
            .filter(|path| {
                !read_allowed(path, policy, &readable) || !write_allowed(path, policy, &writable)
            })
            .cloned()
            .collect();
        let read = read_observed
            .iter()
            .filter(|path| !read_allowed(path, policy, &readable))
            .cloned()
            .collect();
        let write = write_observed
            .iter()
            .filter(|path| !write_allowed(path, policy, &writable))
            .cloned()
            .collect();

        let outbound: Vec<_> = self.outbound.iter().map(endpoint_record_ref).collect();
        let listening: Vec<_> = self.listening.iter().map(endpoint_record_ref).collect();
        let (
            outbound_denied,
            outbound_hostname_check_needed,
            listening_denied,
            listening_not_published,
        ) = match policy.network {
            NetworkPolicy::Allow => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            NetworkPolicy::Deny => (outbound, Vec::new(), listening, Vec::new()),
            // Traces contain IP addresses, while Boxer host rules match DNS
            // names. Report these for review instead of claiming they are gaps.
            NetworkPolicy::Proxy => (
                Vec::new(),
                outbound,
                Vec::new(),
                listening
                    .into_iter()
                    .filter(|endpoint| !policy.publish.contains(&endpoint.port))
                    .collect(),
            ),
        };
        PolicyGaps {
            filesystem: Filesystem {
                read,
                write,
                read_write,
            },
            network: NetworkGaps {
                outbound_denied,
                outbound_hostname_check_needed,
                listening_denied,
                listening_not_published,
            },
        }
    }
}

fn endpoint_record(((address, port), count): ((String, u16), usize)) -> Endpoint {
    Endpoint {
        address,
        port,
        count,
    }
}

fn endpoint_record_ref(((address, port), count): (&(String, u16), &usize)) -> Endpoint {
    Endpoint {
        address: address.clone(),
        port: *port,
        count: *count,
    }
}

fn read_allowed(path: &Path, policy: &Policy, roots: &[PathBuf]) -> bool {
    !covered_by(path, &policy.write_only)
        && (policy.mode == Mode::Unrestricted || covered_by(path, roots))
}

fn write_allowed(path: &Path, policy: &Policy, roots: &[PathBuf]) -> bool {
    (policy.mode == Mode::Unrestricted && !policy.read_only)
        || (policy.mode != Mode::Unrestricted && covered_by(path, roots))
}

fn covered_by(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path == root || (root.is_dir() && path.starts_with(root)))
}

fn parse(contents: &str, directory: &Path) -> Trace {
    let mut trace = Trace::default();
    for line in contents.lines() {
        let line = strip_prefix(line);
        let Some((call, arguments)) = line.split_once('(') else {
            continue;
        };
        let result = line
            .rsplit_once(" = ")
            .map(|(_, result)| result)
            .unwrap_or("");
        if result.starts_with("-1 ") || result == "-1" {
            continue;
        }
        if matches!(call, "connect" | "sendto")
            && let Some((address, port)) = endpoint(arguments)
        {
            *trace.outbound.entry((address, port)).or_default() += 1;
            continue;
        }
        if call == "bind"
            && let Some((address, port)) = endpoint(arguments)
        {
            *trace.listening.entry((address, port)).or_default() += 1;
            continue;
        }
        let paths = quoted_values(arguments);
        let paths = if matches!(call, "symlink" | "symlinkat") {
            paths.into_iter().skip(1).collect::<Vec<_>>()
        } else if matches!(
            call,
            "rename" | "renameat" | "renameat2" | "link" | "linkat"
        ) {
            paths
        } else {
            paths.into_iter().take(1).collect()
        };
        let paths: Vec<_> = paths
            .into_iter()
            .filter_map(|(path, after_path)| {
                resolve_path(path, &arguments[..after_path], directory)
                    .map(|path| (path, after_path))
            })
            .collect();
        if paths.is_empty() {
            continue;
        }
        if matches!(call, "open" | "openat" | "openat2" | "creat") {
            let (path, after_path) = paths.into_iter().next().unwrap();
            let flags = &arguments[after_path..];
            if flags.contains("O_RDWR") {
                trace.read.insert(path.clone());
                trace.write.insert(path);
            } else if flags.contains("O_WRONLY") || flags.contains("O_TRUNC") {
                trace.write.insert(path);
            } else {
                trace.read.insert(path);
            }
        } else if matches!(
            call,
            "mkdir"
                | "mkdirat"
                | "mknod"
                | "mknodat"
                | "rename"
                | "renameat"
                | "renameat2"
                | "rmdir"
                | "unlink"
                | "unlinkat"
                | "link"
                | "linkat"
                | "symlink"
                | "symlinkat"
                | "truncate"
                | "chmod"
                | "fchmodat"
                | "chown"
                | "lchown"
                | "utimensat"
        ) {
            trace.write.extend(paths.into_iter().map(|(path, _)| path));
        } else if call.starts_with("stat")
            || call.starts_with("lstat")
            || call.starts_with("access")
            || call.starts_with("faccess")
            || call.starts_with("readlink")
            || call.starts_with("execve")
            || call == "chdir"
        {
            trace.read.extend(paths.into_iter().map(|(path, _)| path));
        }
    }
    trace
}

fn strip_prefix(line: &str) -> &str {
    if let Some((_, rest)) = line
        .strip_prefix('[')
        .and_then(|line| line.split_once("] "))
    {
        return rest;
    }
    let Some((prefix, rest)) = line.split_once(' ') else {
        return line;
    };
    if prefix.bytes().all(|byte| byte.is_ascii_digit()) {
        rest
    } else {
        line
    }
}

fn first_quoted(input: &str) -> Option<(String, usize)> {
    let mut index = 0;
    while index < input.len() {
        if input.as_bytes()[index] == b'"' {
            index += 1;
            let mut value = String::new();
            while index < input.len() {
                match input.as_bytes()[index] {
                    b'"' => return Some((value, index + 1)),
                    b'\\' => {
                        index += 1;
                        if index >= input.len() {
                            return None;
                        }
                        match input.as_bytes()[index] {
                            b'\\' => value.push('\\'),
                            b'"' => value.push('"'),
                            b'n' => value.push('\n'),
                            b't' => value.push('\t'),
                            byte => value.push(byte as char),
                        }
                        index += 1;
                    }
                    _ => {
                        let character = input[index..].chars().next()?;
                        value.push(character);
                        index += character.len_utf8();
                    }
                }
            }
            return None;
        }
        index += 1;
    }
    None
}

fn quoted_values(input: &str) -> Vec<(String, usize)> {
    let mut values = Vec::new();
    let mut offset = 0;
    while offset < input.len() {
        let Some((value, end)) = first_quoted(&input[offset..]) else {
            break;
        };
        offset += end;
        values.push((value, offset));
    }
    values
}

fn resolve_path(path: String, descriptor: &str, directory: &Path) -> Option<PathBuf> {
    if path.is_empty() || path.as_bytes().contains(&0) {
        return None;
    }
    let path = PathBuf::from(path);
    if path.is_absolute() {
        return Some(path);
    }
    let base = descriptor
        .rsplit_once('<')
        .and_then(|(_, rest)| rest.split_once('>').map(|(path, _)| PathBuf::from(path)))
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| directory.to_owned());
    Some(base.join(path))
}

fn endpoint(arguments: &str) -> Option<(String, u16)> {
    let (_, value) = arguments
        .split_once("sin_port=htons(")
        .or_else(|| arguments.split_once("sin6_port=htons("))?;
    let port = value.split_once(')')?.0.parse().ok()?;
    let address = if let Some((_, tail)) = arguments.split_once("sin_addr=inet_addr(") {
        first_quoted(tail)?.0
    } else if let Some((_, tail)) = arguments.split_once("inet_pton(AF_INET6, ") {
        first_quoted(tail)?.0
    } else {
        return None;
    };
    Some((address, port))
}

fn print_report(report: &Report) {
    println!("Filesystem (read):");
    print_paths(&report.filesystem.read);
    println!("Filesystem (write):");
    print_paths(&report.filesystem.write);
    println!("Filesystem (read+write):");
    print_paths(&report.filesystem.read_write);
    println!("Network (outbound):");
    print_endpoints(&report.network.outbound);
    println!("Network (listening):");
    print_endpoints(&report.network.listening);
    if report.timed_out {
        println!("Command stopped at the configured timeout.");
    } else {
        println!(
            "Command exited with status {}.",
            report.command_exit_code.unwrap_or(1)
        );
    }
    if let Some(gaps) = &report.policy_gaps {
        println!("Filesystem not covered by policy:");
        println!("  Read:");
        print_paths(&gaps.filesystem.read);
        println!("  Write:");
        print_paths(&gaps.filesystem.write);
        println!("  Read and write:");
        print_paths(&gaps.filesystem.read_write);
        println!("Outbound endpoints denied by policy:");
        print_endpoints(&gaps.network.outbound_denied);
        println!("Outbound endpoints to check against hostname rules:");
        print_endpoints(&gaps.network.outbound_hostname_check_needed);
        println!("Listening endpoints denied by policy:");
        print_endpoints(&gaps.network.listening_denied);
        println!("Listening endpoints not published by policy:");
        print_endpoints(&gaps.network.listening_not_published);
    }
}

fn print_paths(paths: &[PathBuf]) {
    if paths.is_empty() {
        println!("  (none)");
    } else {
        for path in paths {
            println!("  {}", path.display());
        }
    }
}

fn print_endpoints(endpoints: &[Endpoint]) {
    if endpoints.is_empty() {
        println!("  (none)");
    } else {
        for endpoint in endpoints {
            println!(
                "  {}:{} ({} {})",
                endpoint.address,
                endpoint.port,
                endpoint.count,
                if endpoint.count == 1 {
                    "access"
                } else {
                    "accesses"
                }
            );
        }
    }
}

fn usage() -> io::Error {
    io::Error::other(
        "Usage: boxer learn [--json] [--timeout SECONDS] [--policy FILE] -- PROGRAM [ARGS...]",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traces_path_modes_and_network_endpoints() {
        let directory = Path::new("/work/project");
        let trace = parse(
            concat!(
                "[pid 123] openat(AT_FDCWD</work/project>, \"input.txt\", O_RDONLY|O_CLOEXEC) = 3\n",
                "[pid 123] openat(AT_FDCWD</work/project>, \"output.txt\", O_WRONLY|O_CREAT|O_CLOEXEC, 0666) = 4\n",
                "[pid 123] openat(AT_FDCWD</work/project>, \"both.txt\", O_RDWR|O_CLOEXEC) = 5\n",
                "[pid 123] connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr(\"203.0.113.8\")}, 16) = 0\n",
                "[pid 123] connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr(\"203.0.113.8\")}, 16) = 0\n",
                "[pid 123] bind(4, {sa_family=AF_INET6, sin6_port=htons(8080), inet_pton(AF_INET6, \"::1\", &sin6_addr)}, 28) = 0\n",
            ),
            directory,
        );
        assert!(trace.read.contains(Path::new("/work/project/input.txt")));
        assert!(trace.write.contains(Path::new("/work/project/output.txt")));
        assert!(trace.read.contains(Path::new("/work/project/both.txt")));
        assert!(trace.write.contains(Path::new("/work/project/both.txt")));
        assert_eq!(trace.outbound.get(&("203.0.113.8".into(), 443)), Some(&2));
        assert_eq!(trace.listening.get(&("::1".into(), 8080)), Some(&1));
    }
}
