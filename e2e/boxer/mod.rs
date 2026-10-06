use solmu_e2e::support::binary;
use std::process::Command;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod audit;
mod cleanup;
mod credentials;
mod environment;
mod explain;
#[cfg(target_os = "linux")]
mod learn;
#[cfg(target_os = "macos")]
mod learn_macos;
mod network;
mod network_profiles;
mod permissions;
mod policies;
mod policy_cli;
mod profiles;
mod readiness;
mod rollback;
mod routes;
mod runtime_groups;
mod sessions;
mod trust;

#[cfg(target_os = "linux")]
mod isolation;
#[cfg(target_os = "linux")]
mod supervisor;

#[cfg(windows)]
mod windows;
