use std::io;

pub fn prepare_network_denial() -> io::Result<()> {
    // A socket hidden in standard I/O could bypass creation/connect controls
    // via ordinary read/write. Treat standard pipes and terminals as explicit
    // caller-provided communication; reject sockets before applying the policy.
    for descriptor in 0..=2 {
        let mut kind: libc::c_int = 0;
        let mut length = std::mem::size_of_val(&kind) as libc::socklen_t;
        // SAFETY: valid, correctly sized output buffers; no descriptor ownership
        // is transferred by getsockopt.
        let result = unsafe {
            libc::getsockopt(
                descriptor,
                libc::SOL_SOCKET,
                libc::SO_TYPE,
                (&mut kind as *mut libc::c_int).cast(),
                &mut length,
            )
        };
        if result == 0 {
            return Err(io::Error::other(
                "--network deny cannot inherit socket-based standard I/O",
            ));
        }
        let error = io::Error::last_os_error();
        if !matches!(
            error.raw_os_error(),
            Some(libc::ENOTSOCK) | Some(libc::EBADF)
        ) {
            return Err(error);
        }
    }
    #[cfg(target_os = "linux")]
    let directory = "/proc/self/fd";
    #[cfg(target_os = "macos")]
    let directory = "/dev/fd";
    // Enumerate before exec, while the launcher is single threaded. Marking
    // close-on-exec preserves Rust's owned handles until launch/error cleanup.
    // It also prevents inherited network and io_uring descriptors surviving exec.
    let descriptors = std::fs::read_dir(directory)?
        .map(|entry| {
            entry.map(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .parse::<libc::c_int>()
                    .ok()
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    for descriptor in descriptors.into_iter().flatten().filter(|fd| *fd >= 3) {
        // SAFETY: fcntl reads/sets flags only; an enumeration descriptor may have
        // closed already. No new files are opened within this loop.
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
        if flags < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EBADF) {
                continue;
            }
            return Err(error);
        }
        if unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
