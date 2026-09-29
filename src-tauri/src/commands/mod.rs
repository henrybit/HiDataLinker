pub mod analysis;
pub mod cache;
pub mod connection;
pub mod llm;
pub mod query;
pub mod schema;
pub mod storage;

pub use analysis::*;
pub use cache::*;
pub use connection::*;
pub use llm::*;
pub use query::*;
pub use schema::*;
pub use storage::*;

use crate::db::LiveEngine;
use crate::error::AppResult;
use crate::runtime::run_db;
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
