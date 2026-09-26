use std::{ffi::OsString, io, process::Command};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[derive(Default)]
struct Policy {
    read_only: bool,
    isolated: bool,
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Solmu sandbox: {error}");
            125
        }
    };
    std::process::exit(code);
}

fn run() -> io::Result<i32> {
    let mut arguments = std::env::args_os().skip(1);
    let mut policy = Policy::default();
    let mut directory = None;
    let mut program: Option<OsString> = None;
    let mut command_arguments = Vec::new();
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            println!(
                "Solmu sandbox\n\nUsage: sandbox [--cwd PATH] [--read-only] [--isolated] [--] [PROGRAM [ARGS...]]\n\nDefault program: solmu-cli\nDefault permissions: filesystem and all network requests allowed.\n--read-only: kernel-enforced filesystem policy on Linux/macOS.\n--isolated: Linux container-style isolation; only the working directory is shared writable. Requires Bubblewrap. Network remains allowed.\nWindows: kernel Job Object contains the process tree; filesystem/network access is allowed."
            );
            return Ok(0);
        } else if argument == "--version" {
            println!("Solmu sandbox {}", env!("CARGO_PKG_VERSION"));
            return Ok(0);
        } else if argument == "--read-only" {
            policy.read_only = true;
        } else if argument == "--isolated" {
            policy.isolated = true;
        } else if argument == "--cwd" {
            directory = Some(
                arguments
                    .next()
                    .ok_or_else(|| io::Error::other("--cwd requires a path"))?,
            );
        } else if argument == "--" {
            program = arguments.next();
            command_arguments.extend(arguments);
            break;
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(io::Error::other(
                "Unknown option; use --help. Put program arguments after --.",
            ));
        } else {
            program = Some(argument);
            command_arguments.extend(arguments);
            break;
        }
    }
    #[cfg(not(target_os = "linux"))]
    if policy.isolated {
        return Err(io::Error::other(
            "--isolated is currently supported only on Linux",
        ));
    }
    let mut command = Command::new(program.unwrap_or_else(|| "solmu-cli".into()));
    command.args(command_arguments);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    platform::run(command, policy)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    pub fn run(_: std::process::Command, _: super::Policy) -> std::io::Result<i32> {
        Err(std::io::Error::other(
            "Kernel sandboxing is unsupported on this OS",
        ))
    }
}
