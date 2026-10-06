use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr,
};
use std::{ffi::OsStr, io, os::unix::process::CommandExt, process::Command};

pub const WORKER: &str = "--boxer-unlink-worker";

pub fn worker() -> io::Result<Option<i32>> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(OsStr::new(WORKER)) {
        return Ok(None);
    }
    let program = arguments
        .next()
        .ok_or_else(|| io::Error::other("Unlink worker requires a program"))?;
    let handled = AccessFs::from_all(ABI::V3);
    let allowed = handled & !AccessFs::RemoveDir & !AccessFs::RemoveFile;
    let mut rules = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(handled)
        .map_err(io::Error::other)?
        .create()
        .map_err(io::Error::other)?;
    rules = rules
        .add_rule(PathBeneath::new(
            PathFd::new("/").map_err(io::Error::other)?,
            allowed,
        ))
        .map_err(io::Error::other)?;
    rules.restrict_self().map_err(io::Error::other)?;
    Err(Command::new(program).args(arguments).exec())
}
