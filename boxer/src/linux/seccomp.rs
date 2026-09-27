use crate::policy::Network;
use std::{
    fs::File,
    io::{self, Seek, Write},
    os::fd::FromRawFd,
};

// Classic BPF over seccomp_data. Reject other ABIs before interpreting syscall
// numbers; x32 shares AUDIT_ARCH_X86_64 but has a separate syscall-number bit.
fn instructions(network: Network) -> io::Result<Vec<libc::sock_filter>> {
    #[cfg(target_arch = "x86_64")]
    let arch = 0xc000003e;
    #[cfg(target_arch = "aarch64")]
    let arch = 0xc00000b7;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    return Err(io::Error::other(
        "Seccomp supports Linux x86_64 and aarch64",
    ));
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        let mut program = Vec::new();
        let mut emit = |code: u16, jt: u8, jf: u8, k: u32| {
            program.push(libc::sock_filter { code, jt, jf, k });
        };
        const LOAD: u16 = 0x20;
        const EQ: u16 = 0x15;
        const RET: u16 = 0x06;
        const DENY: u32 = 0x00050000 | libc::EPERM as u32;
        emit(LOAD, 0, 0, 4);
        emit(EQ, 1, 0, arch);
        emit(RET, 0, 0, 0x80000000); // KILL_PROCESS
        emit(LOAD, 0, 0, 0);
        #[cfg(target_arch = "x86_64")]
        {
            emit(0x35, 0, 1, 0x40000000); // JGE x32 syscall bit
            emit(RET, 0, 0, 0x80000000);
        }
        for syscall in [
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_pivot_root,
            libc::SYS_move_mount,
            libc::SYS_open_tree,
            libc::SYS_fsopen,
            libc::SYS_fsconfig,
            libc::SYS_fsmount,
            libc::SYS_fspick,
            libc::SYS_mount_setattr,
            libc::SYS_setns,
            libc::SYS_unshare,
            libc::SYS_bpf,
            libc::SYS_ptrace,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
            libc::SYS_perf_event_open,
            libc::SYS_userfaultfd,
            libc::SYS_keyctl,
            libc::SYS_add_key,
            libc::SYS_request_key,
            libc::SYS_kexec_load,
            libc::SYS_kexec_file_load,
            libc::SYS_init_module,
            libc::SYS_finit_module,
            libc::SYS_delete_module,
            libc::SYS_reboot,
            libc::SYS_swapon,
            libc::SYS_swapoff,
            libc::SYS_syslog,
            libc::SYS_open_by_handle_at,
            libc::SYS_pidfd_getfd,
            // io_uring can dispatch operations without ordinary syscall checks.
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
        ] {
            emit(EQ, 0, 1, syscall as u32);
            emit(RET, 0, 0, DENY);
        }
        if network == Network::Deny {
            for syscall in [
                libc::SYS_socket,
                libc::SYS_socketpair,
                libc::SYS_connect,
                libc::SYS_bind,
                libc::SYS_listen,
                libc::SYS_accept,
                libc::SYS_accept4,
                libc::SYS_sendto,
                libc::SYS_sendmsg,
                libc::SYS_sendmmsg,
                libc::SYS_recvfrom,
                libc::SYS_recvmsg,
                libc::SYS_recvmmsg,
                libc::SYS_shutdown,
                libc::SYS_setsockopt,
                libc::SYS_getsockopt,
                libc::SYS_getpeername,
                libc::SYS_getsockname,
            ] {
                emit(EQ, 0, 1, syscall as u32);
                emit(RET, 0, 0, DENY);
            }
        }
        if network == Network::Proxy {
            // Shared pathname Unix sockets cross Linux network namespaces.
            // Only IP sockets are created here; the trusted worker receives
            // its preopened broker channels and never passes them to the agent.
            emit(EQ, 0, 1, libc::SYS_socketpair as u32);
            emit(RET, 0, 0, DENY);
            emit(EQ, 0, 5, libc::SYS_socket as u32);
            emit(LOAD, 0, 0, 16);
            emit(EQ, 2, 0, libc::AF_INET as u32);
            emit(EQ, 1, 0, libc::AF_INET6 as u32);
            emit(RET, 0, 0, DENY);
            emit(RET, 0, 0, 0x7fff0000);
        }
        // glibc falls back to clone when clone3 is unavailable. Its pointed-to
        // flags cannot be safely inspected by classic BPF.
        emit(EQ, 0, 1, libc::SYS_clone3 as u32);
        emit(RET, 0, 0, 0x00050000 | libc::ENOSYS as u32);
        emit(EQ, 0, 3, libc::SYS_clone as u32);
        emit(LOAD, 0, 0, 16); // args[0], little-endian ABIs above
        emit(
            0x45,
            0,
            1,
            (libc::CLONE_NEWUSER
                | libc::CLONE_NEWNS
                | libc::CLONE_NEWPID
                | libc::CLONE_NEWNET
                | libc::CLONE_NEWUTS
                | libc::CLONE_NEWIPC
                | libc::CLONE_NEWCGROUP) as u32,
        );
        emit(RET, 0, 0, DENY);
        emit(RET, 0, 0, 0x7fff0000); // ALLOW remaining ordinary execution
        Ok(program)
    }
}

pub fn install_network_denial() -> io::Result<()> {
    let mut instructions = instructions(Network::Deny)?;
    let program = libc::sock_fprog {
        len: instructions.len().try_into().map_err(io::Error::other)?,
        filter: instructions.as_mut_ptr(),
    };
    // SAFETY: the single-threaded launcher supplies a valid, aligned filter
    // buffer for the duration of these synchronous kernel calls. Setting
    // no_new_privs and installing seccomp are irreversible and inherited.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn filter(network: Network) -> io::Result<File> {
    let mut program = Vec::new();
    for instruction in instructions(network)? {
        program.extend(instruction.code.to_ne_bytes());
        program.extend([instruction.jt, instruction.jf]);
        program.extend(instruction.k.to_ne_bytes());
    }
    // SAFETY: constant nul-terminated name; returned descriptor is uniquely owned.
    let fd = unsafe {
        libc::memfd_create(
            c"solmu-seccomp".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.write_all(&program)?;
    file.rewind()?;
    // SAFETY: valid owned descriptor; seals make the filter immutable.
    if unsafe {
        libc::fcntl(
            fd,
            libc::F_ADD_SEALS,
            libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(file)
}
