use super::{AgentTool, Context, OUTPUT_LIMIT, definition};
use futures_util::future::BoxFuture;
use genai::chat::Tool;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

pub struct Bash;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    command: String,
}

impl AgentTool for Bash {
    fn definition(&self) -> Tool {
        definition(
            "Bash",
            "Run a noninteractive Bash command in the thread workspace. A command can access the backend host according to its OS permissions. Execution is limited to 30 seconds; stdout and stderr are bounded.",
            json!({"command":{"type":"string"}}),
            &["command"],
        )
    }
    fn execute(
        &self,
        arguments: Value,
        context: Context,
    ) -> BoxFuture<'static, Result<Value, String>> {
        Box::pin(async move {
            let args: Arguments = serde_json::from_value(arguments)
                .map_err(|error| format!("Invalid Bash arguments: {error}"))?;
            if args.command.is_empty() || args.command.len() > 32_000 {
                return Err("command must contain 1 to 32000 bytes".into());
            }
            let mut command = Command::new(bash());
            command
                .arg("--noprofile")
                .arg("--norc")
                .arg("-c")
                .arg(&args.command)
                .current_dir(&context.workspace)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.as_std_mut().process_group(0);
            }
            #[cfg(windows)]
            {
                command.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
            }
            let mut child = command.spawn().map_err(|error| {
                format!(
                    "Cannot start Bash (install Bash/Git for Windows on the backend host): {error}"
                )
            })?;
            #[cfg(unix)]
            let _tree = ProcessTree(child.id().ok_or("Bash has no process ID")? as i32);
            #[cfg(windows)]
            let _tree = windows::attach(&child)?;
            let stdout = child.stdout.take().ok_or("Missing stdout")?;
            let stderr = child.stderr.take().ok_or("Missing stderr")?;
            let result = tokio::select! {
                _ = context.cancellation.cancelled() => return Err("Tool cancelled".into()),
                result = async { tokio::join!(child.wait(), capture(stdout), capture(stderr)) } => result,
            };
            let status = result.0.map_err(|error| error.to_string())?;
            let (stdout, stdout_truncated) = result.1?;
            let (stderr, stderr_truncated) = result.2?;
            Ok(
                json!({"exit_code":status.code().unwrap_or(1),"stdout":stdout,"stderr":stderr,"truncated":stdout_truncated||stderr_truncated}),
            )
        })
    }
}
fn bash() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("ProgramFiles") {
            let path = PathBuf::from(root).join("Git/bin/bash.exe");
            if path.is_file() {
                return path;
            }
        }
    }
    PathBuf::from("bash")
}
async fn capture(mut pipe: impl AsyncRead + Unpin) -> Result<(String, bool), String> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let count = pipe
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        let keep = count.min((OUTPUT_LIMIT / 2).saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    let text = String::from_utf8_lossy(&output);
    truncated |= text.len() > OUTPUT_LIMIT / 2;
    Ok((
        super::bounded(&text, OUTPUT_LIMIT / 2).to_owned(),
        truncated,
    ))
}
#[cfg(unix)]
struct ProcessTree(i32);
#[cfg(unix)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::{
        mem::{size_of, zeroed},
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
    };
    pub struct Handle(HANDLE);
    // An owned kernel handle has no thread affinity and is only closed once.
    unsafe impl Send for Handle {}
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    pub fn attach(child: &tokio::process::Child) -> Result<Handle, String> {
        unsafe {
            let job = CreateJobObjectW(ptr::null(), ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let job = Handle(job);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let process = child.raw_handle().ok_or("Bash has no process handle")?;
            if AssignProcessToJobObject(job.0, process.cast()) == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let snapshot = Handle(snapshot);
            let mut entry: THREADENTRY32 = zeroed();
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            let mut found = Thread32First(snapshot.0, &mut entry);
            while found != 0 {
                if Some(entry.th32OwnerProcessID) == child.id() {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if thread.is_null() {
                        return Err(std::io::Error::last_os_error().to_string());
                    }
                    let thread = Handle(thread);
                    if ResumeThread(thread.0) == u32::MAX {
                        return Err(std::io::Error::last_os_error().to_string());
                    }
                    return Ok(job);
                }
                found = Thread32Next(snapshot.0, &mut entry);
            }
            Err("Could not resume Bash".into())
        }
    }
}
