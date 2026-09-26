use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};

use axum::http::StatusCode;
use sqlx::SqlitePool;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::error::ApiError;
use crate::llm::Llm;

type ThreadLocks = Arc<Mutex<HashMap<String, Weak<AsyncMutex<()>>>>>;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub llm: Llm,
    locks: ThreadLocks,
}

impl AppState {
    pub fn new(pool: SqlitePool, llm: Llm) -> Self {
        Self {
            pool,
            llm,
            locks: Arc::default(),
        }
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
