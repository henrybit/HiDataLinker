use crate::error::{AppError, AppResult};
use crate::models::QueryLogEntry;
use deadpool_postgres::{Connect, ObjectId};
use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio_postgres::error::DbError;
use tokio_postgres::tls::{MakeTlsConnect, TlsConnect};
use tokio_postgres::{AsyncMessage, Client, Config, Error, SimpleQueryMessage, Socket};

/// Notices kept per connection between two `execute_sql` calls; a runaway
/// `RAISE NOTICE` loop must not grow memory without bound.
const MAX_BUFFERED_NOTICES: usize = 1_000;

#[derive(Default)]
struct NoticeBuffer {
    entries: Mutex<Vec<DbError>>,
    dropped: AtomicUsize,
    closed: AtomicBool,
}

impl NoticeBuffer {
    fn push(&self, notice: DbError) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() < MAX_BUFFERED_NOTICES {
            entries.push(notice);
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn take(&self) -> (Vec<DbError>, usize) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        (
            std::mem::take(&mut *entries),
            self.dropped.swap(0, Ordering::Relaxed),
        )
    }
}

/// Routes server notices from each pooled connection to the `execute_sql`
/// call currently holding it. tokio-postgres has no backend-pid accessor, so
/// connections are keyed by `pg_backend_pid()` and pooled objects are mapped to
/// that pid on first use.
#[derive(Default)]
pub(super) struct NoticeRouter {
    by_pid: Mutex<HashMap<i32, Arc<NoticeBuffer>>>,
    by_object: Mutex<HashMap<ObjectId, Arc<NoticeBuffer>>>,
}

impl NoticeRouter {
    fn register(&self, pid: i32, buffer: Arc<NoticeBuffer>) {
        self.by_pid
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pid, buffer);
    }

    fn unregister(&self, pid: i32, buffer: &Arc<NoticeBuffer>) {
        let mut by_pid = self.by_pid.lock().unwrap_or_else(|e| e.into_inner());
        if by_pid
            .get(&pid)
            .is_some_and(|current| Arc::ptr_eq(current, buffer))
        {
            by_pid.remove(&pid);
        }
    }

    async fn buffer_for(
        &self,
        client: &deadpool_postgres::Client,
    ) -> AppResult<Option<Arc<NoticeBuffer>>> {
        let id = deadpool_postgres::Object::id(client);
        if let Some(buffer) = self
            .by_object
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
        {
            return Ok(Some(buffer.clone()));
        }
        let pid = backend_pid(client).await?;
        let Some(buffer) = self
            .by_pid
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pid)
            .cloned()
        else {
            return Ok(None);
        };
        let mut by_object = self.by_object.lock().unwrap_or_else(|e| e.into_inner());
        by_object.retain(|_, existing| !existing.closed.load(Ordering::Relaxed));
        by_object.insert(id, buffer.clone());
        Ok(Some(buffer))
    }

    /// Drop notices left by earlier statements on this connection and return
    /// a handle that collects the ones raised from now on.
    pub(super) async fn begin(
        &self,
        client: &deadpool_postgres::Client,
    ) -> AppResult<NoticeCapture> {
        let buffer = self.buffer_for(client).await?;
        if let Some(buffer) = &buffer {
            buffer.take();
        }
        Ok(NoticeCapture { buffer })
    }
}

pub(super) struct NoticeCapture {
    buffer: Option<Arc<NoticeBuffer>>,
}

impl NoticeCapture {
    pub(super) fn finish(self) -> Vec<QueryLogEntry> {
        let Some(buffer) = self.buffer else {
            return Vec::new();
        };
        let (notices, dropped) = buffer.take();
        let mut entries: Vec<QueryLogEntry> = notices.iter().map(notice_entry).collect();
        if dropped > 0 {
            entries.push(QueryLogEntry::warning(format!(
                "{dropped} more server message(s) omitted"
            )));
        }
        entries
    }
}

fn notice_entry(notice: &DbError) -> QueryLogEntry {
    let text = format_notice(notice);
    match notice.severity() {
        "WARNING" => QueryLogEntry::warning(text),
        _ => QueryLogEntry::notice(text),
    }
}

pub(super) fn format_notice(notice: &DbError) -> String {
    let mut text = format!("{}: {}", notice.severity(), notice.message());
    if let Some(detail) = notice.detail() {
        text.push_str("\nDETAIL: ");
        text.push_str(detail);
    }
    if let Some(hint) = notice.hint() {
        text.push_str("\nHINT: ");
        text.push_str(hint);
    }
    if let Some(context) = notice.where_().filter(|context| context_is_useful(context)) {
        text.push_str("\nCONTEXT: ");
        text.push_str(context);
    }
    text
}

/// A lone "PL/pgSQL function … at RAISE" frame only restates where the RAISE
/// is; CONTEXT is worth showing when it carries a real call stack.
fn context_is_useful(context: &str) -> bool {
    context.contains('\n') || !context.ends_with(" at RAISE")
}

#[cfg(test)]
mod tests {
    use super::context_is_useful;

    #[test]
    fn hides_single_raise_frame() {
        assert!(!context_is_useful(
            "PL/pgSQL function inline_code_block line 2 at RAISE"
        ));
        assert!(context_is_useful(
            "PL/pgSQL function audit() line 3 at RAISE\nSQL statement \"SELECT audit()\""
        ));
        assert!(context_is_useful("SQL function \"f\" statement 1"));
    }
}

async fn backend_pid(client: &Client) -> AppResult<i32> {
    let messages = client.simple_query("SELECT pg_backend_pid()").await?;
    messages
        .iter()
        .find_map(|message| match message {
            SimpleQueryMessage::Row(row) => row.get(0).and_then(|pid| pid.parse().ok()),
            _ => None,
        })
        .ok_or_else(|| AppError::msg("could not read pg_backend_pid()"))
}

/// deadpool connector that keeps the connection's async messages instead of
/// letting tokio-postgres discard them.
pub(super) struct NoticeConnect<T> {
    pub tls: T,
    pub router: Arc<NoticeRouter>,
}

impl<T> Connect for NoticeConnect<T>
where
    T: MakeTlsConnect<Socket> + Clone + Sync + Send + 'static,
    T::Stream: Sync + Send,
    T::TlsConnect: Sync + Send,
    <T::TlsConnect as TlsConnect<Socket>>::Future: Send,
{
    fn connect(
        &self,
        pg_config: &Config,
    ) -> BoxFuture<'_, Result<(Client, tokio::task::JoinHandle<()>), Error>> {
        let tls = self.tls.clone();
        let pg_config = pg_config.clone();
        let router = self.router.clone();
        Box::pin(async move {
            let (client, mut connection) = pg_config.connect(tls).await?;
            let buffer = Arc::new(NoticeBuffer::default());
            let task_buffer = buffer.clone();
            let pid = Arc::new(Mutex::new(None::<i32>));
            let task_pid = pid.clone();
            let task_router = router.clone();
            let task = tokio::spawn(async move {
                let mut messages =
                    futures_util::stream::poll_fn(move |cx| connection.poll_message(cx));
                while let Some(message) = messages.next().await {
                    match message {
                        Ok(AsyncMessage::Notice(notice)) => task_buffer.push(notice),
                        Ok(_) => {}
                        Err(error) => {
                            log::warn!("postgres connection error: {error}");
                            break;
                        }
                    }
                }
                task_buffer.closed.store(true, Ordering::Relaxed);
                if let Some(pid) = *task_pid.lock().unwrap_or_else(|e| e.into_inner()) {
                    task_router.unregister(pid, &task_buffer);
                }
            });
            if let Ok(backend) = backend_pid(&client).await {
                *pid.lock().unwrap_or_else(|e| e.into_inner()) = Some(backend);
                if !buffer.closed.load(Ordering::Relaxed) {
                    router.register(backend, buffer);
                }
            }
            Ok((client, task))
        })
    }
}
