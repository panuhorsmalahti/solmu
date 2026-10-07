//! Intercepts sandboxed `execve` calls so an operator can approve commands at
//! runtime. The tracer stays outside Bubblewrap's namespaces and seccomp policy.

use std::{
    io,
    process::{Command, ExitStatus},
};

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
use std::{
    collections::HashSet,
    fs,
    io::{BufRead, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::MetadataExt,
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
};

pub fn run(
    command: &mut Command,
    bootstrap: Vec<Vec<u8>>,
    group: &super::cgroup::Group,
    supervisor: &super::proxy::supervisor::Supervisor,
) -> io::Result<ExitStatus> {
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = command;
        let _ = bootstrap;
        let _ = group;
        let _ = supervisor;
        return Err(io::Error::other(
            "Supervised command approvals require Linux x86_64 or aarch64",
        ));
    }
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    run_traced(command, bootstrap, group, supervisor)
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[derive(Clone, Copy)]
enum Decision {
    Once,
    Session,
    Deny,
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct ExecutableIdentity {
    device: u64,
    inode: u64,
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn run_traced(
    command: &mut Command,
    bootstrap: Vec<Vec<u8>>,
    group: &super::cgroup::Group,
    supervisor: &super::proxy::supervisor::Supervisor,
) -> io::Result<ExitStatus> {
    // SAFETY: ptrace is called before exec in the single-threaded launcher
    // child. TRACEME makes the Boxer process the direct tracer of Bubblewrap.
    unsafe {
        command.pre_exec(|| {
            if libc::ptrace(libc::PTRACE_TRACEME, 0, 0, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let root = child.id() as libc::pid_t;
    let mut status = 0;
    if unsafe { libc::waitpid(root, &mut status, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    if !libc::WIFSTOPPED(status) {
        let _ = child.kill();
        return Err(io::Error::other(
            "Bubblewrap exited before command supervision started",
        ));
    }
    let options = libc::PTRACE_O_TRACESECCOMP
        | libc::PTRACE_O_TRACECLONE
        | libc::PTRACE_O_TRACEFORK
        | libc::PTRACE_O_TRACEVFORK
        | libc::PTRACE_O_EXITKILL;
    ptrace(libc::PTRACE_SETOPTIONS, root, options as usize)?;
    ptrace(libc::PTRACE_CONT, root, 0)?;

    let mut bootstrap_grants = bootstrap.into_iter().collect::<HashSet<_>>();
    let mut session_grants = HashSet::<ExecutableIdentity>::new();
    let mut cancelled = 0;
    let root_status = loop {
        let signal = super::INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed);
        if signal != 0 && cancelled == 0 {
            cancelled = signal;
            group.terminate()?;
        }
        let mut status = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::__WALL) };
        if pid < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                let signal = super::INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed);
                if signal != 0 && cancelled == 0 {
                    cancelled = signal;
                    group.terminate()?;
                }
                continue;
            }
            return Err(error);
        }
        if libc::WIFEXITED(status) || libc::WIFSIGNALED(status) {
            if pid == root {
                break status;
            }
            continue;
        }
        if !libc::WIFSTOPPED(status) {
            continue;
        }
        let signal = libc::WSTOPSIG(status);
        let event = status >> 16;
        if event == libc::PTRACE_EVENT_SECCOMP {
            let registers = Registers::get(pid)?;
            let path_address = registers.path_address(pid)?;
            let path = read_remote_string(pid, path_address).unwrap_or_default();
            let identity = executable_identity(pid, &path);
            let decision = if path.is_empty() {
                supervisor.record_command(
                    "<unreadable executable path>",
                    super::proxy::supervisor::Decision::Deny,
                )?;
                Decision::Deny
            } else if bootstrap_grants.remove(&path) {
                Decision::Once
            } else if identity.is_some_and(|identity| session_grants.contains(&identity)) {
                Decision::Session
            } else {
                let mut decision = prompt(&path)?;
                if matches!(decision, Decision::Session) && identity.is_none() {
                    eprintln!(
                        "Boxer could not identify this executable; approval applies once only"
                    );
                    decision = Decision::Once;
                }
                let audit_decision = match decision {
                    Decision::Once => super::proxy::supervisor::Decision::Once,
                    Decision::Session => super::proxy::supervisor::Decision::Session,
                    Decision::Deny => super::proxy::supervisor::Decision::Deny,
                };
                supervisor.record_command(
                    &if path.is_empty() {
                        "<unreadable executable path>".to_owned()
                    } else {
                        String::from_utf8_lossy(&path).into_owned()
                    },
                    audit_decision,
                )?;
                decision
            };
            match decision {
                Decision::Once => {}
                Decision::Session => {
                    session_grants.insert(identity.expect("session grant has an executable ID"));
                }
                Decision::Deny => {
                    eprintln!("Boxer denied command execution");
                    registers.deny(pid)?;
                }
            }
            ptrace(libc::PTRACE_CONT, pid, 0)?;
            continue;
        }
        if event == libc::PTRACE_EVENT_CLONE
            || event == libc::PTRACE_EVENT_FORK
            || event == libc::PTRACE_EVENT_VFORK
        {
            let options = libc::PTRACE_O_TRACESECCOMP
                | libc::PTRACE_O_TRACECLONE
                | libc::PTRACE_O_TRACEFORK
                | libc::PTRACE_O_TRACEVFORK
                | libc::PTRACE_O_EXITKILL;
            ptrace(libc::PTRACE_SETOPTIONS, pid, options as usize)?;
        }
        let deliver = if signal == libc::SIGTRAP || signal == libc::SIGSTOP {
            0
        } else {
            signal
        };
        ptrace(libc::PTRACE_CONT, pid, deliver as usize)?;
    };
    let _ = child.try_wait();
    if cancelled != 0 {
        return Ok(ExitStatus::from_raw((128 + cancelled) << 8));
    }
    status_to_exit(root_status)
}

#[cfg(target_arch = "x86_64")]
struct Registers(libc::user_regs_struct);

#[cfg(target_arch = "aarch64")]
struct Registers([u64; 34]);

#[cfg(target_arch = "x86_64")]
impl Registers {
    fn get(pid: libc::pid_t) -> io::Result<Self> {
        let mut registers = unsafe { std::mem::zeroed::<libc::user_regs_struct>() };
        ptrace_data(
            libc::PTRACE_GETREGS,
            pid,
            (&mut registers as *mut libc::user_regs_struct) as usize,
        )?;
        Ok(Self(registers))
    }

    fn path_address(&self, _pid: libc::pid_t) -> io::Result<u64> {
        let registers = &self.0;
        Ok(if registers.orig_rax as i64 == libc::SYS_execve {
            registers.rdi
        } else {
            registers.rsi
        })
    }

    fn deny(self, pid: libc::pid_t) -> io::Result<()> {
        let mut registers = self.0;
        registers.orig_rax = u64::MAX;
        registers.rax = (-(libc::EACCES as i64)) as u64;
        ptrace_data(
            libc::PTRACE_SETREGS,
            pid,
            (&registers as *const libc::user_regs_struct) as usize,
        )
    }
}

#[cfg(target_arch = "aarch64")]
impl Registers {
    fn get(pid: libc::pid_t) -> io::Result<Self> {
        const NT_PRSTATUS: libc::c_int = 1;
        let mut registers = [0u64; 34];
        let mut vector = libc::iovec {
            iov_base: registers.as_mut_ptr().cast(),
            iov_len: std::mem::size_of_val(&registers),
        };
        if unsafe {
            libc::ptrace(
                libc::PTRACE_GETREGSET,
                pid,
                NT_PRSTATUS as usize as *mut libc::c_void,
                (&mut vector as *mut libc::iovec).cast::<libc::c_void>(),
            )
        } == -1
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(registers))
    }

    fn path_address(&self, _pid: libc::pid_t) -> io::Result<u64> {
        // arm64 passes execve pathname in x0 and execveat pathname in x1.
        Ok(if self.0[8] as i64 == libc::SYS_execve {
            self.0[0]
        } else {
            self.0[1]
        })
    }

    fn deny(self, pid: libc::pid_t) -> io::Result<()> {
        const NT_PRSTATUS: libc::c_int = 1;
        let mut registers = self.0;
        // Setting the syscall number to -1 skips it; x0 is the syscall result.
        registers[8] = u64::MAX;
        registers[0] = (-(libc::EACCES as i64)) as u64;
        let mut vector = libc::iovec {
            iov_base: registers.as_mut_ptr().cast(),
            iov_len: std::mem::size_of_val(&registers),
        };
        if unsafe {
            libc::ptrace(
                libc::PTRACE_SETREGSET,
                pid,
                NT_PRSTATUS as usize as *mut libc::c_void,
                (&mut vector as *mut libc::iovec).cast::<libc::c_void>(),
            )
        } == -1
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn executable_identity(pid: libc::pid_t, path: &[u8]) -> Option<ExecutableIdentity> {
    if path.is_empty() {
        return None;
    }
    let requested = Path::new(std::ffi::OsStr::from_bytes(path));
    let remote = if requested.is_absolute() {
        PathBuf::from(format!("/proc/{pid}/root")).join(requested.strip_prefix("/").ok()?)
    } else {
        PathBuf::from(format!("/proc/{pid}/cwd")).join(requested)
    };
    let metadata = fs::metadata(fs::canonicalize(&remote).ok()?)
        .or_else(|_| {
            if requested.is_absolute() {
                fs::metadata(requested)
            } else {
                Err(io::Error::other("Executable is not visible to Boxer"))
            }
        })
        .ok()?;
    metadata.is_file().then_some(ExecutableIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn prompt(path: &[u8]) -> io::Result<Decision> {
    let display = path.escape_ascii().to_string();
    eprint!(
        "\nBoxer command approval requested: {display}\nAllow once [y], allow for this session [s], or deny [N]? "
    );
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(match answer.trim() {
        "y" | "Y" => Decision::Once,
        "s" | "S" => Decision::Session,
        _ => Decision::Deny,
    })
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn read_remote_string(pid: libc::pid_t, address: u64) -> io::Result<Vec<u8>> {
    let mut value = Vec::with_capacity(256);
    for offset in (0..4096usize).step_by(8) {
        let word = unsafe {
            libc::ptrace(
                libc::PTRACE_PEEKDATA,
                pid,
                (address as usize + offset) as *mut libc::c_void,
                0,
            )
        };
        if word == -1 {
            return Err(io::Error::last_os_error());
        }
        let chunk = (word as u64).to_ne_bytes();
        if let Some(end) = chunk.iter().position(|byte| *byte == 0) {
            value.extend_from_slice(&chunk[..end]);
            return Ok(value);
        }
        value.extend_from_slice(&chunk);
    }
    Err(io::Error::other("Command path exceeds 4096 bytes"))
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn ptrace(request: libc::c_uint, pid: libc::pid_t, data: usize) -> io::Result<()> {
    ptrace_data(request, pid, data)
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn ptrace_data(request: libc::c_uint, pid: libc::pid_t, data: usize) -> io::Result<()> {
    if unsafe { libc::ptrace(request, pid, 0, data) } == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn status_to_exit(status: i32) -> io::Result<ExitStatus> {
    if libc::WIFEXITED(status) {
        Ok(ExitStatus::from_raw(libc::WEXITSTATUS(status) << 8))
    } else if libc::WIFSIGNALED(status) {
        Ok(ExitStatus::from_raw(libc::WTERMSIG(status)))
    } else {
        Err(io::Error::other(
            "Supervised command ended in an invalid state",
        ))
    }
}
