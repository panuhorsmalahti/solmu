use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub enum BuiltinProfile {
    Solmu,
    Codex,
    ClaudeCode,
    OpenCode,
    Pi,
}

#[derive(Clone)]
pub enum Profile {
    Builtin(BuiltinProfile),
    Custom { name: String, path: PathBuf },
}

impl Profile {
    pub fn parse(name: &OsStr) -> io::Result<Self> {
        let Some(name) = name.to_str() else {
            return Err(unknown_profile());
        };
        let builtin = match name {
            "solmu" => Some(BuiltinProfile::Solmu),
            "codex" => Some(BuiltinProfile::Codex),
            "claude-code" | "claude" => Some(BuiltinProfile::ClaudeCode),
            "opencode" | "open-code" => Some(BuiltinProfile::OpenCode),
            "pi" => Some(BuiltinProfile::Pi),
            _ => None,
        };
        if let Some(profile) = builtin {
            return Ok(Self::Builtin(profile));
        }
        if !valid_name(name) {
            return Err(unknown_profile());
        }
        let directory = profile_directory()?;
        let jsonc = directory.join(format!("{name}.jsonc"));
        let path = if jsonc.is_file() {
            jsonc
        } else {
            directory.join(format!("{name}.json"))
        };
        if !path.is_file() {
            return Err(unknown_profile());
        }
        Ok(Self::Custom {
            name: name.to_owned(),
            path,
        })
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Builtin(profile) => profile.name(),
            Self::Custom { name, .. } => name,
        }
    }

    pub fn program(&self) -> Option<&'static str> {
        match self {
            Self::Builtin(profile) => Some(profile.program()),
            Self::Custom { .. } => None,
        }
    }

    pub fn agent(&self) -> Option<crate::policy::AgentProfile> {
        match self {
            Self::Builtin(profile) => profile.agent(),
            Self::Custom { .. } => None,
        }
    }

    pub fn custom_policy(&self) -> Option<&Path> {
        match self {
            Self::Builtin(_) => None,
            Self::Custom { path, .. } => Some(path),
        }
    }

    pub fn is_solmu(&self) -> bool {
        matches!(self, Self::Builtin(BuiltinProfile::Solmu))
    }
}

impl BuiltinProfile {
    fn name(self) -> &'static str {
        match self {
            Self::Solmu => "solmu",
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
        }
    }

    fn program(self) -> &'static str {
        match self {
            Self::Solmu => "solmu",
            Self::Codex => "codex",
            Self::ClaudeCode => "claude",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
        }
    }

    fn agent(self) -> Option<crate::policy::AgentProfile> {
        match self {
            Self::Solmu => None,
            Self::Codex => Some(crate::policy::AgentProfile::Codex),
            Self::ClaudeCode => Some(crate::policy::AgentProfile::ClaudeCode),
            Self::OpenCode => Some(crate::policy::AgentProfile::OpenCode),
            Self::Pi => Some(crate::policy::AgentProfile::Pi),
        }
    }
}

pub fn profile_directory() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("BOXER_PROFILE_DIR") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("Cannot determine the Boxer profile directory"))?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    Ok(config.join("boxer").join("profiles"))
}

pub fn custom_profiles() -> io::Result<Vec<String>> {
    let directory = profile_directory()?;
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if !matches!(
            path.extension(),
            Some(extension) if extension == OsStr::new("json") || extension == OsStr::new("jsonc")
        ) {
            continue;
        }
        if let Some(name) = path.file_stem().and_then(OsStr::to_str)
            && valid_name(name)
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn unknown_profile() -> io::Error {
    io::Error::other(
        "Unknown profile; available built-ins: solmu, codex, claude-code, opencode, pi; custom profiles: BOXER_PROFILE_DIR or ~/.config/boxer/profiles/*.json",
    )
}
