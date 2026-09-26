use crate::{api::error::ApiError, config::optional_env};
use std::path::PathBuf;

pub fn resolve(requested: Option<&str>) -> Result<String, ApiError> {
    let path = if let Some(requested) = requested {
        let path = PathBuf::from(requested);
        if !path.is_absolute() {
            return Err(ApiError::invalid(
                "Workspace must be an absolute directory on the backend host",
            ));
        }
        path
    } else if let Some(path) =
        optional_env("SOLMU_WORKSPACE").map_err(|_| ApiError::invalid("Invalid SOLMU_WORKSPACE"))?
    {
        PathBuf::from(path)
    } else {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .ok_or_else(|| {
                ApiError::invalid("Set SOLMU_WORKSPACE when the backend has no home directory")
            })?;
        PathBuf::from(home).join(".solmu").join("workspace")
    };
    if requested.is_none() {
        std::fs::create_dir_all(&path)
            .map_err(|_| ApiError::invalid("Cannot create the default Solmu workspace"))?;
    }
    let path = path.canonicalize().map_err(|_| {
        ApiError::invalid("Workspace must be an accessible directory on the backend host")
    })?;
    if !path.is_dir() {
        return Err(ApiError::invalid("Workspace must be a directory"));
    }
    Ok(path.to_string_lossy().into_owned())
}
