use serde::Serialize;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Serialize)]
struct Report {
    filesystem: Filesystem,
    network: Network,
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

pub fn command(arguments: &[OsString]) -> io::Result<i32> {
    let mut json = false;
    let mut timeout = None;
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
        } else if arguments[index] == "--policy" || arguments[index] == "--profile" {
            return Err(io::Error::other(
                "Policy comparison for boxer learn is currently available on Linux only",
            ));
        } else {
            return Err(usage());
        }
    }
    if arguments.get(index).is_none_or(|argument| argument != "--")
        || arguments.get(index + 1).is_none()
    {
        return Err(usage());
    }

    let temporary = tempfile::tempdir()?;
    let trace_path = temporary.path().join("fs_usage.log");
    let trace_stderr = temporary.path().join("fs_usage.stderr");
    let child_stdout = temporary.path().join("stdout.log");
    let child_stderr = temporary.path().join("stderr.log");
    let tracer = Command::new("/usr/bin/sudo")
        .args(["-n", "/usr/bin/fs_usage", "-w", "-f", "filesys"])
        .stdout(Stdio::piped())
        .stderr(Stdio::from(std::fs::File::create(&trace_stderr)?))
        .process_group(0)
        .spawn();
    let mut tracer = match tracer {
        Ok(tracer) => tracer,
        Err(error) => {
            return Err(io::Error::other(format!(
                "Cannot start fs_usage. Run `sudo -v` first to authorize filesystem tracing: {error}"
            )));
        }
    };
    std::thread::sleep(Duration::from_millis(75));
    if let Some(status) = tracer.try_wait()? {
        let detail = std::fs::read_to_string(&trace_stderr).unwrap_or_default();
        return Err(io::Error::other(format!(
            "fs_usage exited with {status}; run `sudo -v` to authorize tracing. {detail}"
        )));
    }

    let mut child_command = Command::new(&arguments[index + 1]);
    child_command.args(&arguments[index + 2..]);
    if json {
        child_command
            .stdout(Stdio::from(std::fs::File::create(&child_stdout)?))
            .stderr(Stdio::from(std::fs::File::create(&child_stderr)?));
    }
    let mut child = match child_command.process_group(0).spawn() {
        Ok(child) => child,
        Err(error) => {
            stop_tracer(&mut tracer)?;
            return Err(io::Error::other(format!(
                "Cannot start traced command: {error}"
            )));
        }
    };
    let trace_stdout = tracer
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("fs_usage output was not available"))?;
    let pid = child.id();
    let collector_path = trace_path.clone();
    let collector = std::thread::spawn(move || collect_trace(trace_stdout, &collector_path, pid));
    let status = wait(&mut child, timeout);
    let stop = stop_tracer(&mut tracer);
    collector
        .join()
        .map_err(|_| io::Error::other("fs_usage output collector stopped unexpectedly"))??;
    let (status, timed_out) = status?;
    stop?;
    let filesystem = parse_fs_usage(&std::fs::read_to_string(&trace_path)?, pid);
    let report = Report {
        filesystem,
        network: Network {
            outbound: Vec::new(),
            listening: Vec::new(),
        },
        command_exit_code: status.code(),
        timed_out,
    };
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

fn collect_trace(stdout: impl io::Read, path: &std::path::Path, pid: u32) -> io::Result<()> {
    use std::io::BufRead;
    let mut reader = io::BufReader::new(stdout);
    let mut file = std::fs::File::create(path)?;
    let mut line = String::new();
    let process = format!(".{pid}");
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line
            .split_whitespace()
            .last()
            .is_some_and(|column| column.ends_with(&process))
        {
            use std::io::Write;
            file.write_all(line.as_bytes())?;
        }
    }
    Ok(())
}

fn wait(
    child: &mut std::process::Child,
    timeout: Option<Duration>,
) -> io::Result<(std::process::ExitStatus, bool)> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status, false));
        }
        if timeout.is_some_and(|timeout| start.elapsed() >= timeout) {
            // SAFETY: process_group(0) assigned a private group to this command.
            unsafe { libc::kill(-(child.id() as i32), libc::SIGTERM) };
            std::thread::sleep(Duration::from_millis(250));
            if child.try_wait()?.is_none() {
                // SAFETY: this targets only the command's private process group.
                unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            }
            return child.wait().map(|status| (status, true));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn stop_tracer(tracer: &mut std::process::Child) -> io::Result<()> {
    if tracer.try_wait()?.is_none() {
        // SAFETY: process_group(0) assigned a private group to sudo/fs_usage.
        unsafe { libc::kill(-(tracer.id() as i32), libc::SIGINT) };
        let start = Instant::now();
        while tracer.try_wait()?.is_none() && start.elapsed() < Duration::from_millis(500) {
            std::thread::sleep(Duration::from_millis(10));
        }
        if tracer.try_wait()?.is_none() {
            // SAFETY: this targets only the tracer's private process group.
            unsafe { libc::kill(-(tracer.id() as i32), libc::SIGKILL) };
        }
    }
    tracer.wait().map(|_| ())
}

fn parse_fs_usage(contents: &str, pid: u32) -> Filesystem {
    let mut read = BTreeSet::new();
    let mut write = BTreeSet::new();
    let process = format!(".{pid}");
    for line in contents.lines() {
        let columns: Vec<_> = line.split_whitespace().collect();
        if columns.len() < 5
            || !columns
                .last()
                .is_some_and(|column| column.ends_with(&process))
        {
            continue;
        }
        let operation = columns[1];
        let end = columns.len().saturating_sub(2);
        let mut start = 2;
        if columns
            .get(start)
            .is_some_and(|column| column.starts_with("F="))
        {
            start += 1;
        }
        if start >= end {
            continue;
        }
        let path = columns[start..end].join(" ");
        if !path.starts_with('/') {
            continue;
        }
        let path = PathBuf::from(path);
        if is_write_operation(operation) {
            write.insert(path);
        } else if is_read_operation(operation) {
            read.insert(path);
        }
    }
    let read_write: BTreeSet<_> = read.intersection(&write).cloned().collect();
    let read = read.difference(&write).cloned().collect();
    let write = write.difference(&read_write).cloned().collect();
    let read_write = read_write.into_iter().collect();
    Filesystem {
        read,
        write,
        read_write,
    }
}

fn is_read_operation(operation: &str) -> bool {
    matches!(
        operation,
        "open"
            | "openat"
            | "stat"
            | "stat64"
            | "lstat"
            | "lstat64"
            | "access"
            | "readlink"
            | "RdData"
            | "read"
            | "read_nocancel"
            | "pread"
            | "pread_nocancel"
            | "getattrlist"
            | "getattrlistbulk"
            | "PageIn"
            | "PAGE_IN"
    )
}

fn is_write_operation(operation: &str) -> bool {
    matches!(
        operation,
        "WrData"
            | "write"
            | "write_nocancel"
            | "pwrite"
            | "pwrite_nocancel"
            | "truncate"
            | "rename"
            | "renameat"
            | "unlink"
            | "unlinkat"
            | "mkdir"
            | "rmdir"
            | "remove"
            | "chmod"
            | "WrMeta"
            | "setattrlist"
            | "PageOut"
            | "PAGE_OUT"
    )
}

fn copy_to_stderr(path: &std::path::Path) -> io::Result<()> {
    let mut file = std::fs::File::open(path)?;
    let stderr = io::stderr();
    io::copy(&mut file, &mut stderr.lock())?;
    Ok(())
}

fn print_report(report: &Report) {
    println!("Filesystem (read):");
    print_paths(&report.filesystem.read);
    println!("Filesystem (write):");
    print_paths(&report.filesystem.write);
    println!("Filesystem (read+write):");
    print_paths(&report.filesystem.read_write);
    println!("Network (outbound):\n  (not available from fs_usage)");
    println!("Network (listening):\n  (not available from fs_usage)");
    if report.timed_out {
        println!("Command stopped at the configured timeout.");
    } else {
        println!(
            "Command exit code: {}",
            report.command_exit_code.unwrap_or(1)
        );
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

fn usage() -> io::Error {
    io::Error::other("Usage: boxer learn [--json] [--timeout SECONDS] -- PROGRAM [ARGS...]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fs_usage_paths_for_the_requested_process() {
        let report = parse_fs_usage(
            concat!(
                "14:56:52.386 open F=58 /tmp/solmu/input.txt 0.000012 probe.42\n",
                "14:56:52.400 RdData F=58 /tmp/solmu/input.txt 0.000011 probe.42\n",
                "14:56:52.410 WrData F=59 /tmp/solmu/output.txt 0.000020 probe.42\n",
                "14:56:52.420 WrData F=59 /tmp/other.txt 0.000020 other.42\n",
            ),
            42,
        );
        assert_eq!(report.read, vec![PathBuf::from("/tmp/solmu/input.txt")]);
        assert_eq!(report.write, vec![PathBuf::from("/tmp/solmu/output.txt")]);
        assert!(report.read_write.is_empty());
    }
}
