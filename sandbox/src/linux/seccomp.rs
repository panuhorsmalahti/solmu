use std::{
    fs::File,
    io::{self, Seek, Write},
    os::fd::FromRawFd,
};

// Classic BPF over seccomp_data. Reject other ABIs before interpreting syscall
// numbers; x32 shares AUDIT_ARCH_X86_64 but has a separate syscall-number bit.
pub fn filter() -> io::Result<File> {
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
            program.extend(code.to_ne_bytes());
            program.extend([jt, jf]);
            program.extend(k.to_ne_bytes());
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
            // io_uring can dispatch operations without ordinary syscall checks.
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
        ] {
            emit(EQ, 0, 1, syscall as u32);
            emit(RET, 0, 0, DENY);
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
        emit(RET, 0, 0, 0x7fff0000); // ALLOW ordinary execution and networking
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
}
