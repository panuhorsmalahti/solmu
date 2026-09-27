mod guest;
mod host;
mod ipc;

pub use guest::worker;
pub use host::Host;

use crate::network::Target;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Worker {
    pub bridge: i32,
    pub inbound: i32,
    pub arguments: Vec<OsString>,
    pub local: Vec<Target>,
}

pub const WORKER: &str = "--boxer-network-worker";
