use std::{
    ffi::{OsStr, OsString},
    io,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{
        CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, PROCESS_INFORMATION,
        STARTUPINFOW,
    },
};

fn quote(value: &OsStr) -> Vec<u16> {
    let mut output = vec![b'"' as u16];
    let mut slashes = 0;
    for ch in value.encode_wide() {
        if ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if ch == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        output.push(ch);
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    output
}

// Rust Command inherits other inheritable handles on Windows. A daemon must
// inherit none: otherwise a launching client's pipes or ConPTY remain open.
pub fn spawn(directory: &Path, name: &str, directories: &[PathBuf]) -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let mut command = quote(executable.as_os_str());
    let mut arguments = vec![OsString::from("--server"), OsString::from(name)];
    for cwd in directories {
        arguments.push("--cwd".into());
        arguments.push(cwd.as_os_str().to_owned());
    }
    for arg in arguments {
        command.push(b' ' as u16);
        command.extend(quote(&arg));
    }
    command.push(0);
    let application: Vec<_> = executable.as_os_str().encode_wide().chain([0]).collect();
    let mut environment: Vec<_> = std::env::vars_os()
        .filter(|(key, _)| {
            !key.to_string_lossy()
                .eq_ignore_ascii_case("SOLMU_MUXER_DIR")
                && !key.to_string_lossy().eq_ignore_ascii_case("TERM")
        })
        .collect();
    environment.push(("SOLMU_MUXER_DIR".into(), directory.as_os_str().to_owned()));
    environment.push(("TERM".into(), "xterm-256color".into()));
    environment.sort_by_key(|(key, _)| key.to_string_lossy().to_uppercase());
    let mut block = Vec::new();
    for (key, value) in environment {
        block.extend(key.encode_wide());
        block.push(b'=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // All buffers remain live for CreateProcessW; startup has no inherited
    // standard handles and the child gets its own ConPTYs for Solmu panes.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            block.as_ptr().cast(),
            std::ptr::null(),
            &startup,
            &mut process,
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }
    Ok(())
}
