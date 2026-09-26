use std::{env, error::Error, net::SocketAddr};

pub struct Config {
    pub bind_addr: SocketAddr,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            bind_addr: optional_env("SOLMU_BIND_ADDR")?
                .unwrap_or_else(|| "127.0.0.1:3000".into())
                .parse()?,
            database_url: optional_env("SOLMU_DATABASE_URL")?
                .unwrap_or_else(|| "sqlite://solmu.db".into()),
        })
    }
}

pub fn optional_env(name: &str) -> Result<Option<String>, env::VarError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value.trim().to_owned())),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error),
    }
}
