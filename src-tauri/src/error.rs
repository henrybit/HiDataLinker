use std::error::Error as StdError;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("connection not found: {0}")]
    NotConnected(String),
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error(transparent)]
    Mysql(#[from] mysql_async::Error),
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[error(transparent)]
    SqlServer(#[from] tiberius::error::Error),
    #[error(transparent)]
    Oracle(#[from] oracle_rs::Error),
    #[error("{0}")]
    PgPool(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl AppError {
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }

    /// Human-readable message for the UI / IPC layer.
    ///
    /// `tokio_postgres::Error`'s Display for SQL failures is only `"db error"`;
    /// the real Postgres text lives on the nested `DbError`.
    pub fn user_message(&self) -> String {
        match self {
            Self::Postgres(error) => format_postgres_error(error),
            other => other.to_string(),
        }
    }
}

fn format_postgres_error(error: &tokio_postgres::Error) -> String {
    if let Some(db) = error.as_db_error() {
        let mut message = db.to_string();
        let code = db.code().code();
        if !code.is_empty() {
            message.push_str(" (");
            message.push_str(code);
            message.push(')');
        }
        return message;
    }

    let mut message = error.to_string();
    let mut current = StdError::source(error);
    while let Some(source) = current {
        message.push_str(": ");
        message.push_str(&source.to_string());
        current = source.source();
    }
    message
}

impl From<deadpool_postgres::PoolError> for AppError {
    fn from(error: deadpool_postgres::PoolError) -> Self {
        Self::PgPool(error.to_string())
    }
}

impl From<deadpool_postgres::CreatePoolError> for AppError {
    fn from(error: deadpool_postgres::CreatePoolError) -> Self {
        Self::PgPool(error.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.user_message())
    }
}

pub type AppResult<T> = Result<T, AppError>;
