pub mod messages;
pub mod profile;
pub mod threads;

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
