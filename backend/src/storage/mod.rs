pub mod messages;
pub mod profile;
pub mod scheduled_tasks;
pub mod threads;
pub mod tools;
pub mod usage;

#[derive(Debug)]
pub enum StoreError {
    NotFound(&'static str),
    Conflict(&'static str),
    Database(sqlx::Error),
}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}
