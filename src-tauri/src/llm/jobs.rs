//! Background model calls. The command that starts one returns immediately.
//! The webview polls for the outcome so a slow or timed-out request does not
//! fail the whole Tauri invoke.

use super::{complete, LlmCompleteRequest};
use crate::error::{AppError, AppResult};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

const JOB_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_JOBS: usize = 32;

enum Phase {
    Pending,
    Done(Value),
    Failed(String),
}

struct Job {
    phase: Phase,
    updated: Instant,
}

struct Jobs {
    inner: Mutex<HashMap<String, Job>>,
}

impl Jobs {
    fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Job>> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn insert(&self, id: &str) {
        let mut jobs = self.lock();
        sweep(&mut jobs);
        jobs.insert(
            id.to_string(),
            Job {
                phase: Phase::Pending,
                updated: Instant::now(),
            },
        );
    }

    fn finish(&self, id: &str, outcome: AppResult<Value>) {
        let mut jobs = self.lock();
        let Some(job) = jobs.get_mut(id) else {
            return;
        };
        job.phase = match outcome {
            Ok(value) => Phase::Done(value),
            Err(error) => Phase::Failed(error.to_string()),
        };
        job.updated = Instant::now();
    }

    fn poll(&self, id: &str) -> AppResult<LlmJobStatus> {
        if Uuid::parse_str(id).is_err() {
            return Err(AppError::msg("invalid model job"));
        }
        let mut jobs = self.lock();
        let Some(job) = jobs.get(id) else {
            return Err(AppError::msg("model job not found"));
        };
        match &job.phase {
            Phase::Pending => Ok(LlmJobStatus {
                status: "pending".to_string(),
                result: None,
                error: None,
            }),
            Phase::Done(value) => {
                let result = value.clone();
                jobs.remove(id);
                Ok(LlmJobStatus {
                    status: "done".to_string(),
                    result: Some(result),
                    error: None,
                })
            }
            Phase::Failed(message) => {
                let error = message.clone();
                jobs.remove(id);
                Ok(LlmJobStatus {
                    status: "failed".to_string(),
                    result: None,
                    error: Some(error),
                })
            }
        }
    }
}

fn sweep(jobs: &mut HashMap<String, Job>) {
    let now = Instant::now();
    jobs.retain(|_, job| now.duration_since(job.updated) < JOB_TTL);
    while jobs.len() > MAX_JOBS {
        let oldest = jobs
            .iter()
            .min_by_key(|(_, job)| job.updated)
            .map(|(id, _)| id.clone());
        let Some(id) = oldest else {
            break;
        };
        jobs.remove(&id);
    }
}

fn registry() -> &'static Jobs {
    static JOBS: std::sync::OnceLock<Jobs> = std::sync::OnceLock::new();
    JOBS.get_or_init(Jobs::new)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmJobStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Queue the model call on the worker pool and return its id.
pub fn start(request: LlmCompleteRequest) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    registry().insert(&id);
    let task_id = id.clone();
    crate::runtime::handle().spawn(async move {
        registry().finish(&task_id, complete(request).await);
    });
    Ok(id)
}

/// Read a queued model call. A finished job is removed after it is read.
pub fn poll(id: &str) -> AppResult<LlmJobStatus> {
    registry().poll(id.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pending_job_stays_until_it_finishes_and_is_read_once() {
        let jobs = Jobs::new();
        let id = "11111111-1111-4111-8111-111111111111";
        jobs.insert(id);
        assert_eq!(jobs.poll(id).unwrap().status, "pending");

        jobs.finish(id, Ok(json!({"relations": []})));
        let done = jobs.poll(id).unwrap();
        assert_eq!(done.status, "done");
        assert_eq!(done.result, Some(json!({"relations": []})));
        assert!(jobs.poll(id).is_err());
    }

    #[test]
    fn failed_job_returns_the_error_text() {
        let jobs = Jobs::new();
        let id = "22222222-2222-4222-8222-222222222222";
        jobs.insert(id);
        jobs.finish(id, Err(AppError::msg("model request timed out")));
        let failed = jobs.poll(id).unwrap();
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error.as_deref(), Some("model request timed out"));
    }

    #[test]
    fn rejects_an_id_that_is_not_a_uuid() {
        let jobs = Jobs::new();
        let error = jobs.poll("../secret").unwrap_err();
        assert!(error.to_string().contains("invalid model job"));
    }
}
