use crate::{
    Policy,
    policy::{self, Mode},
};
use serde::Serialize;
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
struct Explanation {
    path: PathBuf,
    operation: &'static str,
    result: &'static str,
    reason: &'static str,
    mode: Mode,
}

pub fn run(
    policy: &Policy,
    workspace: &Path,
    requested: &Path,
    operation: &str,
) -> io::Result<i32> {
    let path = canonical_target(workspace, requested)?;
    let (result, reason) = if (cfg!(windows)
        && (policy.mode != Mode::Unrestricted || policy.read_only))
        || (policy.isolated && !cfg!(target_os = "linux"))
    {
        (
            "unsupported",
            "This Boxer backend does not enforce the requested filesystem policy on this platform",
        )
    } else if policy.mode == Mode::Unrestricted {
        if operation == "write" && policy.read_only {
            ("denied", "--read-only denies writes")
        } else {
            ("allowed", "Unrestricted mode allows filesystem access")
        }
    } else if operation == "write" && policy.read_only {
        (
            "denied",
            "--read-only makes the workspace and read grants non-writable",
        )
    } else if operation == "write" && covered_by(&path, &policy.write) {
        (
            "allowed",
            "The path is covered by an explicit writable grant",
        )
    } else if operation == "write" && path.starts_with(workspace) {
        ("allowed", "The path is inside the writable workspace")
    } else if operation == "read"
        && (path.starts_with(workspace)
            || covered_by(&path, &policy.read)
            || covered_by(&path, &policy.write))
    {
        (
            "allowed",
            "The path is inside the workspace or an explicit path grant",
        )
    } else if operation == "read" && runtime_allows(&path, policy.mode) {
        (
            "allowed",
            "The path is part of the read-only runtime files exposed by this mode",
        )
    } else if operation == "read" && covered_by(&path, &policy::device_paths()) {
        (
            "allowed",
            "The path is a device file required by the platform sandbox",
        )
    } else {
        (
            "denied",
            "The resolved filesystem allowlist contains no grant for this path and operation",
        )
    };
    let explanation = Explanation {
        path,
        operation: if operation == "read" { "read" } else { "write" },
        result,
        reason,
        mode: policy.mode,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&explanation).map_err(io::Error::other)?
    );
    Ok(0)
}

fn canonical_target(workspace: &Path, requested: &Path) -> io::Result<PathBuf> {
    let absolute = if requested.is_absolute() {
        requested.to_owned()
    } else {
        workspace.join(requested)
    };
    let mut ancestor = absolute.as_path();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        let name = ancestor
            .file_name()
            .ok_or_else(|| io::Error::other("Cannot resolve the requested path"))?;
        suffix.push(name.to_owned());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| io::Error::other("Cannot resolve the requested path"))?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for component in suffix.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn covered_by(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path == root || (root.is_dir() && path.starts_with(root)))
}

fn runtime_allows(path: &Path, mode: Mode) -> bool {
    let runtime: Vec<PathBuf> = match mode {
        Mode::Unrestricted => Vec::new(),
        Mode::Workspace => policy::runtime_paths(),
        Mode::Isolated => policy::ISOLATED_RUNTIME.iter().map(PathBuf::from).collect(),
    };
    covered_by(path, &runtime)
}
