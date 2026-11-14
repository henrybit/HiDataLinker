pub mod agents;
pub mod analysis;
pub mod cache;
pub mod connection;
pub mod import;
pub mod llm;
pub mod migration;
pub mod query;
pub mod schema;
pub mod sql_audit;
pub mod storage;

pub use agents::*;
pub use analysis::*;
pub use cache::*;
pub use connection::*;
pub use import::*;
pub use llm::*;
pub use migration::*;
pub use query::*;
pub use schema::*;
pub use sql_audit::*;
pub use storage::*;

use crate::db::LiveEngine;
use crate::error::AppResult;
use crate::runtime::run_db;
use crate::sql_audit::AuditSource;
use crate::state::AppState;
use std::future::Future;

pub async fn with_engine<T, F, Fut>(
    state: &AppState,
    connection_id: String,
    work: F,
) -> AppResult<T>
where
    F: FnOnce(LiveEngine) -> Fut + Send + 'static,
    Fut: Future<Output = AppResult<T>> + Send + 'static,
    T: Send + 'static,
{
    let engine = state.engine(&connection_id)?;
    run_db(work(engine)).await
}

/// Like [`with_engine`], tagging audited statements with the feature that issued them.
pub async fn with_engine_as<T, F, Fut>(
    state: &AppState,
    connection_id: String,
    source: AuditSource,
    work: F,
) -> AppResult<T>
where
    F: FnOnce(LiveEngine) -> Fut + Send + 'static,
    Fut: Future<Output = AppResult<T>> + Send + 'static,
    T: Send + 'static,
{
    with_engine_tagged(state, connection_id, source, None, work).await
}

/// Like [`with_engine_as`], with a free-form detail stored next to the source.
pub async fn with_engine_tagged<T, F, Fut>(
    state: &AppState,
    connection_id: String,
    source: AuditSource,
    detail: Option<String>,
    work: F,
) -> AppResult<T>
where
    F: FnOnce(LiveEngine) -> Fut + Send + 'static,
    Fut: Future<Output = AppResult<T>> + Send + 'static,
    T: Send + 'static,
{
    let engine = state.engine(&connection_id)?.with_source(source, detail);
    run_db(work(engine)).await
}
