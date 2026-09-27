use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};

use axum::http::StatusCode;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
pub struct Change {
    #[serde(rename = "type")]
    kind: &'static str,
    thread_id: Option<String>,
}
impl Change {
    pub fn skills(id: &str) -> Self {
        Self {
            kind: "skills_changed",
            thread_id: Some(id.to_owned()),
        }
    }
}

use super::error::ApiError;
use crate::llm::Llm;

type ThreadLocks = Arc<Mutex<HashMap<String, Weak<AsyncMutex<()>>>>>;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub llm: Llm,
    pub tools: crate::tools::Registry,
    pub skills: crate::skills::Manager,
    locks: ThreadLocks,
    pub events: broadcast::Sender<Change>,
    responses: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, llm: Llm) -> Self {
        let events = broadcast::channel(256).0;
        Self {
            pool,
            llm,
            tools: crate::tools::Registry::new(),
            locks: Arc::default(),
            skills: crate::skills::Manager::new(events.clone()),
            events,
            responses: Arc::default(),
        }
    }

    pub fn changed(&self, id: &str) {
        let _ = self.events.send(Change {
            kind: "conversation_changed",
            thread_id: Some(id.to_owned()),
        });
    }

    pub fn profile_changed(&self) {
        let _ = self.events.send(Change {
            kind: "profile_changed",
            thread_id: None,
        });
    }

    pub fn resync() -> Change {
        Change {
            kind: "conversation_changed",
            thread_id: None,
        }
    }

    pub fn response(&self, id: &str) -> Result<ResponsePermit, ApiError> {
        let guard = self.lock_thread(id)?;
        let token = CancellationToken::new();
        self.responses
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id.to_owned(), token.clone());
        Ok(ResponsePermit {
            state: self.clone(),
            id: id.to_owned(),
            token,
            _guard: guard,
        })
    }

    pub fn stop(&self, id: &str) {
        if let Some(token) = self
            .responses
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(id)
        {
            token.cancel();
        }
    }

    pub async fn wait_stopped(&self, id: &str) -> Result<(), ApiError> {
        tokio::time::timeout(std::time::Duration::from_secs(35), async {
            while self
                .responses
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .contains_key(id)
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::GATEWAY_TIMEOUT,
                "stop_pending",
                "The tool is still finishing; please wait",
            )
        })
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
        self.state
            .responses
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.id);
    }
}
