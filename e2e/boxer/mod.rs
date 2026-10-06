use solmu_e2e::support::binary;
use std::process::Command;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod audit;
mod explain;
mod network;
mod permissions;
mod policies;
mod profiles;
mod readiness;
mod rollback;
mod routes;

#[cfg(target_os = "linux")]
mod isolation;

#[cfg(windows)]
mod windows;
