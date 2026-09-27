use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Launch {
    #[default]
    Solmu,
    Shell {
        #[serde(default)]
        argv: Vec<String>,
    },
    Command {
        argv: Vec<String>,
    },
}
impl Launch {
    pub fn solmu(&self) -> bool {
        matches!(self, Self::Solmu)
    }
    pub fn validate(&self) -> Result<(), String> {
        let argv = match self {
            Self::Solmu => return Ok(()),
            Self::Shell { argv } if argv.is_empty() => return Ok(()),
            Self::Shell { argv } | Self::Command { argv } => argv,
        };
        if argv.is_empty()
            || argv[0].trim().is_empty()
            || argv.len() > 128
            || argv.iter().any(|value| value.contains('\0'))
            || argv.iter().map(String::len).sum::<usize>() > 65536
        {
            return Err(
                "Launch needs an executable and at most 128 arguments / 64 KiB, without NUL bytes"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn script(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Err("Enter a command".into());
        }
        let argv = if cfg!(windows) {
            vec![
                std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()),
                "/d".into(),
                "/c".into(),
                text.into(),
            ]
        } else {
            vec!["/bin/sh".into(), "-c".into(), text.into()]
        };
        let value = Self::Command { argv };
        value.validate()?;
        Ok(value)
    }
    pub fn title(&self) -> String {
        match self {
            Self::Solmu => "Solmu".into(),
            Self::Shell { .. } => "Shell".into(),
            Self::Command { argv } => argv
                .first()
                .and_then(|program| std::path::Path::new(program).file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Command".into()),
        }
    }
}
