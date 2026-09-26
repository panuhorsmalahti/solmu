use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};

use axum::http::StatusCode;
use sqlx::SqlitePool;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::sync::broadcast;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
pub struct Change {
    #[serde(rename = "type")]
    kind: &'static str,
    thread_id: Option<String>,
}

use super::error::ApiError;
use crate::llm::Llm;

type ThreadLocks = Arc<Mutex<HashMap<String, Weak<AsyncMutex<()>>>>>;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub llm: Llm,
    locks: ThreadLocks,
    pub events: broadcast::Sender<Change>,
    responses: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, llm: Llm) -> Self {
        Self {
            pool,
            llm,
            locks: Arc::default(),
            events: broadcast::channel(256).0,
            responses: Arc::default(),
        }
    }

    pub fn changed(&self, id: &str) {
        let _ = self.events.send(Change { kind: "conversation_changed", thread_id: Some(id.to_owned()) });
    }

    pub fn resync() -> Change { Change { kind: "conversation_changed", thread_id: None } }

    pub fn response(&self, id: &str) -> Result<ResponsePermit, ApiError> {
        let guard = self.lock_thread(id)?;
        let token = CancellationToken::new();
        self.responses.lock().unwrap_or_else(|error| error.into_inner()).insert(id.to_owned(), token.clone());
        Ok(ResponsePermit { state: self.clone(), id: id.to_owned(), token, _guard: guard })
    }

    pub fn stop(&self, id: &str) {
        if let Some(token) = self.responses.lock().unwrap_or_else(|error| error.into_inner()).get(id) { token.cancel(); }
    }

    pub fn lock_thread(&self, id: &str) -> Result<OwnedMutexGuard<()>, ApiError> {
        let lock = {
            let mut locks = self.locks.lock().unwrap_or_else(|error| error.into_inner());
            locks.retain(|_, weak| weak.strong_count() > 0);
            if let Some(lock) = locks.get(id).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(AsyncMutex::new(()));
                locks.insert(id.to_owned(), Arc::downgrade(&lock));
                lock
            }
        };
        lock.try_lock_owned().map_err(|_| {
            ApiError::new(
                StatusCode::CONFLICT,
                "thread_busy",
                "This thread is currently processing a response",
            )
        })
    }
}

pub struct ResponsePermit {
    state: AppState,
    id: String,
    pub token: CancellationToken,
    _guard: OwnedMutexGuard<()>,
}
impl Drop for ResponsePermit {
    fn drop(&mut self) {
        self.token.cancel();
        self.state.responses.lock().unwrap_or_else(|error| error.into_inner()).remove(&self.id);
    }
}
