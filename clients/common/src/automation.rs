//! Private, process-scoped Solmu CLI control protocol. It is not a backend API.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

pub const MAX_FRAME: usize = 1024 * 1024;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub token: String,
    pub instance: String,
    pub thread: String,
    pub action: Action,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Prompt {
        text: String,
    },
    Wait {
        turn: Option<String>,
        timeout_ms: Option<u64>,
    },
    Turn {
        turn: String,
    },
    Stop,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Turn {
    pub id: String,
    pub thread: String,
    pub state: String,
    pub user_message: Option<String>,
    pub assistant_message: Option<String>,
    pub text: Option<String>,
    pub truncated: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
    pub accepted: Option<bool>,
    pub turn: Option<String>,
}
impl Failure {
    pub fn new(
        code: &str,
        message: impl Into<String>,
        accepted: Option<bool>,
        turn: Option<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            accepted,
            turn,
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}
#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Accepted { turn: Turn },
    Completed { turn: Turn },
    Idle,
    Stopping,
    Error { failure: Failure },
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Metadata {
    pub version: u32,
    pub instance: String,
    pub port: u16,
    pub thread: Option<String>,
    pub ready: bool,
    pub queued: usize,
    pub turn: Option<TurnSummary>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct TurnSummary {
    pub id: String,
    pub state: String,
}
pub fn validate_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty()
        || text.len() > 65536
        || text
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err(
            "Prompts must contain 1 through 65536 UTF-8 bytes of text without control keys".into(),
        );
    }
    Ok(())
}
pub fn write<T: Serialize>(output: &mut impl Write, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::other("Native Solmu frame exceeds 1 MiB"));
    }
    output.write_all(&(bytes.len() as u32).to_be_bytes())?;
    output.write_all(&bytes)
}
pub fn read<T: DeserializeOwned>(input: &mut impl Read) -> io::Result<T> {
    let mut size = [0; 4];
    input.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > MAX_FRAME {
        return Err(io::Error::other("Native Solmu frame exceeds 1 MiB"));
    }
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
