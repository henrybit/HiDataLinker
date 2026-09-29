use crate::error::{AppError, AppResult};
use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::{Builder, Handle, Runtime};

static DB_RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn worker_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(4, 16))
        .unwrap_or(8)
}

fn runtime() -> &'static Runtime {
    DB_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(worker_threads())
            .thread_name("db-worker")
            .enable_all()
            .build()
            .expect("failed to start database worker runtime")
    })
}

pub fn handle() -> Handle {
    runtime().handle().clone()
}

/// Run async work on the dedicated multi-thread pool so Tauri's UI/IPC loop stays free.
pub async fn run_worker<F, T>(fut: F) -> AppResult<T>
where
    F: Future<Output = AppResult<T>> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle().spawn(async move {
        let _ = tx.send(fut.await);
    });
    rx.await
        .map_err(|error| AppError::msg(format!("worker cancelled: {error}")))?
}

/// Database calls share the same worker pool as other async I/O.
pub async fn run_db<F, T>(fut: F) -> AppResult<T>
where
    F: Future<Output = AppResult<T>> + Send + 'static,
    T: Send + 'static,
{
    run_worker(fut).await
}

pub async fn run_blocking<T, F>(work: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    let handle = handle();
    handle
        .spawn_blocking(work)
        .await
        .map_err(|error| AppError::msg(format!("blocking worker cancelled: {error}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_work_off_the_caller_runtime() {
        let thread = run_db(async {
            Ok(std::thread::current()
                .name()
                .unwrap_or_default()
                .to_string())
        })
        .await
        .expect("db worker");
        assert!(
            thread.starts_with("db-worker"),
            "expected db-worker thread, got {thread}"
        );
    }
}
