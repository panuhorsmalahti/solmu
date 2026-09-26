use crate::Policy;
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::ffi::OsStrExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct Group {
    path: PathBuf,
    pub processes: File,
}

impl Group {
    pub fn create(policy: &Policy) -> io::Result<Self> {
        let current = fs::read_to_string("/proc/self/cgroup")?;
        let relative = current
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .ok_or_else(|| io::Error::other("Linux isolation requires cgroup v2"))?;
        let current = Path::new("/sys/fs/cgroup")
            .join(relative.trim_start_matches('/'))
            .canonicalize()?;
        let requested = policy
            .cgroup_root
            .clone()
            .or_else(|| std::env::var_os("SOLMU_CGROUP_ROOT").map(PathBuf::from));
        let root = match requested {
            Some(path) => path.canonicalize()?,
            None => current.ancestors().find(|path| delegated(path)).map(PathBuf::from)
                .ok_or_else(|| io::Error::other("No delegated cgroup v2 parent. Start with systemd-run -p Delegate=yes -p DelegateSubgroup=supervisor, or set SOLMU_CGROUP_ROOT (see docs/sandbox.md)."))?,
        };
        let name = CString::new(root.as_os_str().as_bytes()).map_err(io::Error::other)?;
        let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: valid pathname and output buffer, initialized on success.
        if unsafe { libc::statfs(name.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { stat.assume_init() }.f_type != libc::CGROUP2_SUPER_MAGIC {
            return Err(io::Error::other(
                "--cgroup-root must be a real cgroup v2 directory",
            ));
        }
        if !current.starts_with(&root) || current == root {
            return Err(io::Error::other(
                "Launcher must run in a child of the delegated cgroup parent (use DelegateSubgroup=supervisor)",
            ));
        }
        if !fs::read_to_string(root.join("cgroup.procs"))?
            .trim()
            .is_empty()
        {
            return Err(io::Error::other(
                "Delegated cgroup parent must contain no processes",
            ));
        }
        let available = fs::read_to_string(root.join("cgroup.controllers"))?;
        for controller in ["cpu", "memory", "pids"] {
            if !available.split_whitespace().any(|item| item == controller) {
                return Err(io::Error::other(format!(
                    "Delegated cgroup lacks {controller} controller"
                )));
            }
        }
        fs::write(root.join("cgroup.subtree_control"), "+cpu +memory +pids")?;
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let path = root.join(format!("solmu-{}-{id}", std::process::id()));
        fs::create_dir(&path)?;
        let configured = (|| {
            fs::write(
                path.join("cpu.max"),
                format!("{} 100000", u64::from(policy.cpus.unwrap_or(2)) * 100000),
            )?;
            fs::write(
                path.join("memory.max"),
                (u64::from(policy.memory_mib.unwrap_or(2048)) * 1024 * 1024).to_string(),
            )?;
            fs::write(path.join("memory.swap.max"), "0")?;
            fs::write(path.join("memory.oom.group"), "1")?;
            fs::write(
                path.join("pids.max"),
                policy.pids.unwrap_or(256).to_string(),
            )?;
            // Require cleanup support before launching anything.
            OpenOptions::new()
                .write(true)
                .open(path.join("cgroup.kill"))?;
            OpenOptions::new()
                .write(true)
                .open(path.join("cgroup.procs"))
        })();
        match configured {
            Ok(processes) => Ok(Self { path, processes }),
            Err(error) => {
                let _ = fs::remove_dir(&path);
                Err(error)
            }
        }
    }

    pub fn cleanup(&self) -> io::Result<()> {
        fs::write(self.path.join("cgroup.kill"), "1")?;
        for _ in 0..100 {
            if fs::read_to_string(self.path.join("cgroup.events"))?.contains("populated 0") {
                return fs::remove_dir(&self.path);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(io::Error::other(
            "Sandbox cgroup still contains processes after termination",
        ))
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = self.cleanup();
        }
    }
}

fn delegated(path: &Path) -> bool {
    let Ok(name) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut value = 0u8;
    // SAFETY: valid buffers; systemd marks delegated parents with this xattr.
    unsafe {
        libc::getxattr(
            name.as_ptr(),
            c"user.delegate".as_ptr(),
            (&mut value as *mut u8).cast(),
            1,
        ) == 1
            && value == b'1'
    }
}

pub fn attach(fd: i32) -> io::Result<()> {
    // Only async-signal-safe calls here: invoked between fork and exec. Writing
    // 0 attaches this child before Bubblewrap can fork or execute the agent.
    loop {
        let written = unsafe { libc::write(fd, b"0\n".as_ptr().cast(), 2) };
        if written == 2 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

impl AsRawFd for Group {
    fn as_raw_fd(&self) -> i32 {
        self.processes.as_raw_fd()
    }
}
