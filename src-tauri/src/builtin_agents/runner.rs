use crate::agent::catalog::{self, ToolError};
use crate::builtin_agents::context::{
    context_event_payload, prepare_conversation_context, CompressLlm,
};
use crate::builtin_agents::store::{
    append_transcript, save_checkpoint, TranscriptEntry,
};
use crate::error::{AppError, AppResult};
use crate::llm::{complete, LlmCompleteRequest};
use crate::runtime::handle;
use crate::state::AppState;
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

const MAX_CONCURRENT_RUNS: usize = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLlmConfig {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStartRunRequest {
    pub agent_id: String,
    pub session_id: String,
    pub query: String,
    pub system_md: String,
    /// App UI locale (`zh` | `en`); forwarded to the Node runtime for LLM output language.
    #[serde(default = "default_locale")]
    pub locale: String,
    /// Optional @-mentioned data sources; empty/omitted = all saved connections.
    #[serde(default)]
    pub connection_ids: Vec<String>,
    /// Optional /-selected skills; empty/omitted = full tool set.
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub llm: AgentLlmConfig,
}

fn default_locale() -> String {
    "en".into()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStartRunResponse {
    pub run_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunnerStatus {
    pub ready: bool,
    pub active_runs: usize,
    pub script_path: Option<String>,
    pub node_bin: Option<String>,
}

#[derive(Clone)]
struct RunCtx {
    agent_id: String,
    session_id: String,
    llm: AgentLlmConfig,
    cancelled: Arc<AtomicBool>,
    started_at: Instant,
}

struct NdjsonMsg {
    id: String,
    type_: String,
    payload: Value,
}

pub struct AgentRunner {
    app: AppHandle,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    ready: AtomicBool,
    runs: RwLock<HashMap<String, RunCtx>>,
    script_path: RwLock<Option<PathBuf>>,
    node_bin: RwLock<Option<PathBuf>>,
}

impl AgentRunner {
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app,
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            ready: AtomicBool::new(false),
            runs: RwLock::new(HashMap::new()),
            script_path: RwLock::new(None),
            node_bin: RwLock::new(None),
        })
    }

    fn app_state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }

    pub async fn status(&self) -> AgentRunnerStatus {
        let runs = self.runs.read().await;
        AgentRunnerStatus {
            ready: self.ready.load(Ordering::SeqCst),
            active_runs: runs.len(),
            script_path: self
                .script_path
                .read()
                .await
                .as_ref()
                .map(|p| p.display().to_string()),
            node_bin: self
                .node_bin
                .read()
                .await
                .as_ref()
                .map(|p| p.display().to_string()),
        }
    }

    pub async fn ensure_started(self: &Arc<Self>) -> AppResult<()> {
        if self.ready.load(Ordering::SeqCst) {
            let mut guard = self.child.lock().await;
            if let Some(child) = guard.as_mut() {
                match child.try_wait() {
                    Ok(None) => return Ok(()),
                    Ok(Some(status)) => {
                        log::warn!("agents-runtime exited: {status}; restarting");
                        *guard = None;
                        self.ready.store(false, Ordering::SeqCst);
                    }
                    Err(err) => {
                        log::warn!("agents-runtime wait error: {err}; restarting");
                        *guard = None;
                        self.ready.store(false, Ordering::SeqCst);
                    }
                }
            } else {
                self.ready.store(false, Ordering::SeqCst);
            }
        }
        self.spawn_runtime().await
    }

    async fn spawn_runtime(self: &Arc<Self>) -> AppResult<()> {
        let script = resolve_runtime_script(&self.app)?;
        let node = resolve_node_bin(&self.app)?;
        *self.script_path.write().await = Some(script.clone());
        *self.node_bin.write().await = Some(node.clone());

        let mut child = Command::new(&node)
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| {
                AppError::msg(format!(
                    "failed to start agents-runtime (node={node:?} script={script:?}): {err}"
                ))
            })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::msg("agents-runtime missing stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::msg("agents-runtime missing stderr"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::msg("agents-runtime missing stdin"))?;

        *self.child.lock().await = Some(child);
        *self.stdin.lock().await = Some(stdin);

        let runner = Arc::clone(self);
        handle().spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Err(err) = runner.handle_line(&line).await {
                    log::warn!("agents-runtime line error: {}", err.user_message());
                }
            }
            runner.ready.store(false, Ordering::SeqCst);
            log::warn!("agents-runtime stdout closed");
        });

        handle().spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::info!("[agents-runtime] {line}");
            }
        });

        // Wait briefly for runtime.ready
        for _ in 0..50 {
            if self.ready.load(Ordering::SeqCst) {
                return Ok(());
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        // Allow start even if ready frame was missed; first message may still work.
        self.ready.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn send_json(&self, value: Value) -> AppResult<()> {
        let mut line = value.to_string();
        line.push('\n');
        let mut guard = self.stdin.lock().await;
        let stdin = guard
            .as_mut()
            .ok_or_else(|| AppError::msg("agents-runtime stdin unavailable"))?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|err| AppError::msg(format!("agents-runtime write failed: {err}")))?;
        stdin
            .flush()
            .await
            .map_err(|err| AppError::msg(format!("agents-runtime flush failed: {err}")))?;
        Ok(())
    }

    async fn handle_line(self: &Arc<Self>, line: &str) -> AppResult<()> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let raw: Value = serde_json::from_str(trimmed)?;
        let id = raw
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let type_ = raw
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let payload = raw.get("payload").cloned().unwrap_or(Value::Null);
        let msg = NdjsonMsg { id, type_, payload };
        match msg.type_.as_str() {
            "runtime.ready" => {
                self.ready.store(true, Ordering::SeqCst);
            }
            "tool.invoke" => {
                let runner = Arc::clone(self);
                handle().spawn(async move {
                    if let Err(err) = runner.handle_tool_invoke(msg).await {
                        log::warn!("tool.invoke failed: {}", err.user_message());
                    }
                });
            }
            "llm.complete" => {
                let runner = Arc::clone(self);
                handle().spawn(async move {
                    if let Err(err) = runner.handle_llm_complete(msg).await {
                        log::warn!("llm.complete failed: {}", err.user_message());
                    }
                });
            }
            "run.event" => self.handle_run_event(&msg.payload).await?,
            "run.done" | "run.error" => self.handle_run_done(&msg.payload).await?,
            "pong" => {}
            other => log::debug!("agents-runtime unknown type: {other}"),
        }
        Ok(())
    }

    async fn handle_tool_invoke(self: &Arc<Self>, msg: NdjsonMsg) -> AppResult<()> {
        let run_id = msg
            .payload
            .get("runId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let name = msg
            .payload
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let args = msg
            .payload
            .get("args")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let cancelled = {
            let runs = self.runs.read().await;
            runs.get(&run_id)
                .map(|r| r.cancelled.load(Ordering::SeqCst))
                .unwrap_or(true)
        };
        let reply = if cancelled {
            json!({ "ok": false, "error": "cancelled", "result": null })
        } else {
            match catalog::invoke(&self.app_state(), &name, &args).await {
                Ok(result) => json!({ "ok": true, "result": result, "error": null }),
                Err(ToolError::Unknown) => {
                    json!({ "ok": false, "error": "unknown tool", "result": null })
                }
                Err(ToolError::BadRequest(message)) | Err(ToolError::Failed(message)) => {
                    json!({ "ok": false, "error": message, "result": null })
                }
            }
        };
        self.send_json(json!({
            "id": msg.id,
            "type": "tool.result",
            "payload": reply
        }))
        .await
    }

    async fn handle_llm_complete(self: &Arc<Self>, msg: NdjsonMsg) -> AppResult<()> {
        let run_id = msg
            .payload
            .get("runId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let runs = self.runs.read().await;
        let Some(ctx) = runs.get(&run_id).cloned() else {
            drop(runs);
            return self
                .send_json(json!({
                    "id": msg.id,
                    "type": "llm.result",
                    "payload": { "ok": false, "error": "unknown run", "result": null }
                }))
                .await;
        };
        drop(runs);

        if ctx.cancelled.load(Ordering::SeqCst) {
            return self
                .send_json(json!({
                    "id": msg.id,
                    "type": "llm.result",
                    "payload": { "ok": false, "error": "cancelled", "result": null }
                }))
                .await;
        }

        let system = msg
            .payload
            .get("system")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let human = msg
            .payload
            .get("human")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let schema = msg
            .payload
            .get("schema")
            .cloned()
            .unwrap_or_else(|| json!({}));
        // Prefer host-held credentials; Node may echo provider/model/baseUrl only.
        let provider = msg
            .payload
            .get("provider")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(ctx.llm.provider.as_str())
            .to_string();
        let model = msg
            .payload
            .get("model")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(ctx.llm.model.as_str())
            .to_string();
        let base_url = msg
            .payload
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or(ctx.llm.base_url.as_str())
            .to_string();

        let request = LlmCompleteRequest {
            provider,
            api_key: ctx.llm.api_key.clone(),
            model,
            base_url,
            system,
            human,
            schema,
        };

        let reply = match complete(request).await {
            Ok(result) => json!({ "ok": true, "result": result, "error": null }),
            Err(err) => json!({
                "ok": false,
                "result": null,
                "error": err.user_message()
            }),
        };
        self.send_json(json!({
            "id": msg.id,
            "type": "llm.result",
            "payload": reply
        }))
        .await
    }

    async fn handle_run_event(&self, payload: &Value) -> AppResult<()> {
        let run_id = payload
            .get("runId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let event = payload.get("event").cloned().unwrap_or(Value::Null);
        let runs = self.runs.read().await;
        let Some(ctx) = runs.get(&run_id).cloned() else {
            return Ok(());
        };
        drop(runs);

        let _ = persist_event(&self.app_state(), &ctx, &event).await;

        let _ = self.app.emit(
            "agent-run-event",
            json!({
                "runId": run_id,
                "agentId": ctx.agent_id,
                "sessionId": ctx.session_id,
                "event": event
            }),
        );
        Ok(())
    }

    async fn handle_run_done(&self, payload: &Value) -> AppResult<()> {
        let run_id = payload
            .get("runId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let markdown = payload
            .get("markdown")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let error = payload
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        let ctx = {
            let mut runs = self.runs.write().await;
            runs.remove(&run_id)
        };
        let Some(ctx) = ctx else {
            return Ok(());
        };

        let chats_dir = self.app_state().chats_dir().clone();
        let agent_id = ctx.agent_id.clone();
        let session_id = ctx.session_id.clone();
        let duration_ms = ctx.started_at.elapsed().as_millis() as u64;
        let md = markdown.clone();
        let err = error.clone();
        let status = if err.as_deref() == Some("cancelled") {
            "cancelled"
        } else if err.is_some() && md.is_empty() {
            "error"
        } else {
            "done"
        };
        let run_meta = json!({ "durationMs": duration_ms });

        let _ = crate::runtime::run_blocking({
            let chats_dir = chats_dir.clone();
            let agent_id = agent_id.clone();
            let session_id = session_id.clone();
            let md = md.clone();
            let err = err.clone();
            let run_meta = run_meta.clone();
            move || {
                if !md.is_empty() {
                    let _ = append_transcript(
                        &chats_dir,
                        &agent_id,
                        &session_id,
                        TranscriptEntry {
                            id: Uuid::new_v4().to_string(),
                            at: now_iso(),
                            role: "assistant".into(),
                            kind: "markdown".into(),
                            content: md.clone(),
                            meta: Some(run_meta.clone()),
                        },
                    );
                } else if let Some(ref message) = err {
                    let _ = append_transcript(
                        &chats_dir,
                        &agent_id,
                        &session_id,
                        TranscriptEntry {
                            id: Uuid::new_v4().to_string(),
                            at: now_iso(),
                            role: "assistant".into(),
                            kind: "error".into(),
                            content: message.clone(),
                            meta: Some(run_meta.clone()),
                        },
                    );
                }
                let checkpoint = json!({
                    "markdown": md,
                    "error": err,
                    "durationMs": duration_ms,
                });
                save_checkpoint(
                    &chats_dir,
                    &agent_id,
                    &session_id,
                    checkpoint,
                    Some(status.to_string()),
                )
            }
        })
        .await;

        let _ = self.app.emit(
            "agent-run-done",
            json!({
                "runId": run_id,
                "agentId": agent_id,
                "sessionId": session_id,
                "markdown": markdown,
                "error": error,
                "status": status,
                "durationMs": duration_ms
            }),
        );
        Ok(())
    }

    pub async fn start_run(self: &Arc<Self>, request: AgentStartRunRequest) -> AppResult<String> {
        self.ensure_started().await?;
        {
            let runs = self.runs.read().await;
            if runs.len() >= MAX_CONCURRENT_RUNS {
                return Err(AppError::msg(format!(
                    "concurrency limit ({MAX_CONCURRENT_RUNS}) reached"
                )));
            }
        }

        let run_id = Uuid::new_v4().to_string();
        let ctx = RunCtx {
            agent_id: request.agent_id.clone(),
            session_id: request.session_id.clone(),
            llm: request.llm.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
            started_at: Instant::now(),
        };
        self.runs.write().await.insert(run_id.clone(), ctx);

        // Return runId immediately so the UI thread is not blocked by schema refresh /
        // DB warm-up. Graph start continues on the worker runtime.
        let runner = Arc::clone(self);
        let run_id_bg = run_id.clone();
        handle().spawn(async move {
            if let Err(error) = runner.launch_run(run_id_bg, request).await {
                log::warn!("agent run launch failed: {}", error.user_message());
            }
        });
        Ok(run_id)
    }

    async fn launch_run(self: &Arc<Self>, run_id: String, request: AgentStartRunRequest) -> AppResult<()> {
        // Bail out if the user cancelled while we were still warming schema cache.
        {
            let runs = self.runs.read().await;
            if runs
                .get(&run_id)
                .map(|ctx| ctx.cancelled.load(Ordering::SeqCst))
                .unwrap_or(true)
            {
                return Ok(());
            }
        }

        let locale = if request.locale.trim() == "zh" {
            "zh"
        } else {
            "en"
        };

        let connection_ids = request
            .connection_ids
            .iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        let connection_filter = if connection_ids.is_empty() {
            None
        } else {
            Some(connection_ids.as_slice())
        };

        let schema_digest = match crate::schema_cache::ensure_and_digest(
            &self.app_state(),
            connection_filter,
        )
        .await
        {
            Ok(digest) => digest,
            Err(error) => {
                log::warn!(
                    "schema cache ensure failed before run: {}",
                    error.user_message()
                );
                String::new()
            }
        };

        {
            let runs = self.runs.read().await;
            if runs
                .get(&run_id)
                .map(|ctx| ctx.cancelled.load(Ordering::SeqCst))
                .unwrap_or(true)
            {
                return Ok(());
            }
        }

        let compress_llm = CompressLlm {
            provider: request.llm.provider.clone(),
            api_key: request.llm.api_key.clone(),
            model: request.llm.model.clone(),
            base_url: request.llm.base_url.clone(),
            locale: locale.to_string(),
        };
        let conversation = match prepare_conversation_context(
            self.app_state().chats_dir(),
            &request.agent_id,
            &request.session_id,
            Some(&compress_llm),
        )
        .await
        {
            Ok(ctx) => ctx,
            Err(error) => {
                log::warn!(
                    "conversation context prepare failed: {}",
                    error.user_message()
                );
                crate::builtin_agents::ConversationContext {
                    text: String::new(),
                    compressed: false,
                    summary_chars: 0,
                    recent_chars: 0,
                }
            }
        };
        if conversation.compressed {
            let _ = self
                .app
                .emit(
                    "agent-run-event",
                    json!({
                        "runId": run_id,
                        "agentId": request.agent_id,
                        "sessionId": request.session_id,
                        "event": context_event_payload(&conversation)
                    }),
                );
        }

        // Never send apiKey to Node.
        let payload = json!({
            "runId": run_id,
            "agentId": request.agent_id,
            "sessionId": request.session_id,
            "query": request.query,
            "systemMd": request.system_md,
            "locale": locale,
            "schemaDigest": schema_digest,
            "connectionIds": connection_ids,
            "skillIds": request.skill_ids,
            "conversationContext": conversation.text,
            "llm": {
                "provider": request.llm.provider,
                "model": request.llm.model,
                "baseUrl": request.llm.base_url
            }
        });
        if let Err(err) = self
            .send_json(json!({
                "id": Uuid::new_v4().to_string(),
                "type": "run.start",
                "payload": payload
            }))
            .await
        {
            let agent_id = request.agent_id.clone();
            let session_id = request.session_id.clone();
            self.runs.write().await.remove(&run_id);
            let message = err.user_message();
            let _ = self.app.emit(
                "agent-run-done",
                json!({
                    "runId": run_id,
                    "agentId": agent_id,
                    "sessionId": session_id,
                    "markdown": "",
                    "error": message,
                    "status": "error"
                }),
            );
            return Err(err);
        }
        Ok(())
    }

    pub async fn cancel_run(&self, run_id: &str) -> AppResult<()> {
        {
            let runs = self.runs.read().await;
            if let Some(ctx) = runs.get(run_id) {
                ctx.cancelled.store(true, Ordering::SeqCst);
            }
        }
        self.send_json(json!({
            "id": Uuid::new_v4().to_string(),
            "type": "run.cancel",
            "payload": { "runId": run_id }
        }))
        .await
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

async fn persist_event(state: &AppState, ctx: &RunCtx, event: &Value) -> AppResult<()> {
    let typ = event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let chats_dir = state.chats_dir().clone();
    let agent_id = ctx.agent_id.clone();
    let session_id = ctx.session_id.clone();
    let event = event.clone();

    crate::runtime::run_blocking(move || {
        // Live "status" ticks stay UI-only; transcript keeps plan/tool/step milestones.
        let entry = match typ.as_str() {
            "plan" => {
                let steps = event.get("steps").cloned().unwrap_or(json!([]));
                let content = steps
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .enumerate()
                            .map(|(i, step)| {
                                let title = step
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Step");
                                format!("{}. {title}", i + 1)
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                Some(TranscriptEntry {
                    id: Uuid::new_v4().to_string(),
                    at: now_iso(),
                    role: "assistant".into(),
                    kind: "progress".into(),
                    content: format!("Plan\n{content}"),
                    meta: Some(json!({ "phase": "plan", "steps": steps })),
                })
            }
            "step-start" => {
                let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                let total = event.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                let title = event
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Step");
                Some(TranscriptEntry {
                    id: Uuid::new_v4().to_string(),
                    at: now_iso(),
                    role: "assistant".into(),
                    kind: "progress".into(),
                    content: format!("Step {}/{}: {title}", index + 1, total),
                    meta: Some(json!({ "phase": "step", "index": index, "total": total })),
                })
            }
            "tool" => {
                let tool = event
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool");
                let args = event.get("args").cloned().unwrap_or(json!({}));
                let summary = event
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let sql = args.get("sql").and_then(|v| v.as_str()).unwrap_or("");
                let ok = event.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let duration_ms = event.get("durationMs").cloned().unwrap_or(json!(0));
                Some(TranscriptEntry {
                    id: Uuid::new_v4().to_string(),
                    at: now_iso(),
                    role: "assistant".into(),
                    kind: "tool".into(),
                    content: if sql.is_empty() {
                        summary.to_string()
                    } else {
                        sql.to_string()
                    },
                    meta: Some(json!({
                        "tool": tool,
                        "ok": ok,
                        "durationMs": duration_ms,
                        "args": args,
                        "summary": summary
                    })),
                })
            }
            "step-done" => {
                let note = event
                    .get("note")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                Some(TranscriptEntry {
                    id: Uuid::new_v4().to_string(),
                    at: now_iso(),
                    role: "assistant".into(),
                    kind: "progress".into(),
                    content: note.to_string(),
                    meta: Some(json!({ "phase": "step-done", "index": index })),
                })
            }
            "error" => {
                let message = event
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("error");
                Some(TranscriptEntry {
                    id: Uuid::new_v4().to_string(),
                    at: now_iso(),
                    role: "assistant".into(),
                    kind: "error".into(),
                    content: message.to_string(),
                    meta: Some(json!({ "phase": "error" })),
                })
            }
            // final is persisted on run.done as markdown
            _ => None,
        };

        if let Some(entry) = entry {
            append_transcript(&chats_dir, &agent_id, &session_id, entry)?;
        }
        Ok(())
    })
    .await
}

fn is_runnable_file(path: &Path) -> bool {
    path.is_file()
}

fn node_file_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["node.exe", "node"]
    }
    #[cfg(not(windows))]
    {
        &["node"]
    }
}

fn push_if_missing(out: &mut Vec<PathBuf>, path: PathBuf) {
    if !out.iter().any(|item| item == &path) {
        out.push(path);
    }
}

fn candidate_node_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut out = Vec::new();

    // 1) Tauri externalBin lands next to the executable (…/Contents/MacOS/node).
    if let Ok(exe_dir) = app.path().executable_dir() {
        for name in node_file_names() {
            push_if_missing(&mut out, exe_dir.join(name));
        }
    }

    // 2) Resources (optional copy).
    if let Ok(resource_dir) = app.path().resource_dir() {
        for rel in [
            "node",
            "bin/node",
            "agents-runtime/node",
            "node.exe",
            "bin/node.exe",
            "agents-runtime/node.exe",
        ] {
            push_if_missing(&mut out, resource_dir.join(rel));
        }
    }

    // 3) PATH entries (GUI apps often have a minimal PATH).
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        for name in node_file_names() {
            push_if_missing(&mut out, dir.join(name));
        }
    }

    // 4) Common absolute installs (Homebrew / system).
    for dir in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        r"C:\Program Files\nodejs",
    ] {
        for name in node_file_names() {
            push_if_missing(&mut out, PathBuf::from(dir).join(name));
        }
    }

    // 5) Version managers under $HOME (nvm / fnm / volta / asdf).
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }) {
        for name in node_file_names() {
            push_if_missing(&mut out, home.join(".volta").join("bin").join(name));
            push_if_missing(&mut out, home.join(".local").join("bin").join(name));
            push_if_missing(
                &mut out,
                home.join(".asdf").join("shims").join(name),
            );
        }

        // nvm: ~/.nvm/versions/node/<ver>/bin/node — pick lexicographically latest.
        let nvm_versions = home.join(".nvm").join("versions").join("node");
        if let Ok(entries) = std::fs::read_dir(&nvm_versions) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| path.is_dir())
                .collect();
            versions.sort();
            for version_dir in versions.into_iter().rev() {
                for name in node_file_names() {
                    push_if_missing(&mut out, version_dir.join("bin").join(name));
                }
            }
        }

        // fnm: ~/.fnm/node-versions/<ver>/installation/bin/node
        let fnm_versions = home.join(".fnm").join("node-versions");
        if let Ok(entries) = std::fs::read_dir(&fnm_versions) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| path.is_dir())
                .collect();
            versions.sort();
            for version_dir in versions.into_iter().rev() {
                for name in node_file_names() {
                    push_if_missing(
                        &mut out,
                        version_dir.join("installation").join("bin").join(name),
                    );
                }
            }
        }
    }

    out
}

fn resolve_node_bin(app: &AppHandle) -> AppResult<PathBuf> {
    if let Ok(custom) = std::env::var("HIDATALINKER_NODE") {
        let path = PathBuf::from(custom);
        if is_runnable_file(&path) {
            return Ok(path);
        }
        return Err(AppError::msg(format!(
            "HIDATALINKER_NODE not found: {}",
            path.display()
        )));
    }

    for candidate in candidate_node_paths(app) {
        if is_runnable_file(&candidate) {
            return Ok(candidate);
        }
    }

    Err(AppError::msg(
        "Node.js runtime not found. Packaged builds ship a bundled node next to the app binary; \
         for local/dev installs set HIDATALINKER_NODE or install Node on PATH \
         (Homebrew/nvm). Rebuild with `pnpm fetch:bundled-node && pnpm tauri:build`.",
    ))
}

fn resolve_runtime_script(app: &AppHandle) -> AppResult<PathBuf> {
    if let Ok(custom) = std::env::var("HIDATALINKER_AGENTS_RUNTIME") {
        let path = PathBuf::from(custom);
        if path.is_file() {
            return Ok(path);
        }
        return Err(AppError::msg(format!(
            "HIDATALINKER_AGENTS_RUNTIME not found: {}",
            path.display()
        )));
    }

    // Packaged resources (see tauri.conf.json bundle.resources)
    if let Ok(resource_dir) = app.path().resource_dir() {
        for candidate in [
            resource_dir.join("agents-runtime").join("main.mjs"),
            resource_dir.join("main.mjs"),
        ] {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    // Dev: relative to src-tauri crate
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev = manifest
        .join("..")
        .join("agents-runtime")
        .join("dist")
        .join("main.mjs");
    if let Ok(canonical) = dunce_canonicalize(&dev) {
        if canonical.is_file() {
            return Ok(canonical);
        }
    }

    Err(AppError::msg(
        "agents-runtime/dist/main.mjs not found; run `pnpm --filter @hidatalinker/agents-runtime build`",
    ))
}

fn dunce_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}
