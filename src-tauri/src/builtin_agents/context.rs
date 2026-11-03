use crate::builtin_agents::store::{load_session, TranscriptEntry};
use crate::error::{AppError, AppResult};
use crate::llm::{complete, LlmCompleteRequest};
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Keep the newest messages verbatim up to this many characters.
const KEEP_RECENT_CHARS: usize = 8_000;
/// Compress when older+recent context would exceed this.
const COMPRESS_TRIGGER_CHARS: usize = 12_000;
/// Cap for the rolling summary text.
const MAX_SUMMARY_CHARS: usize = 4_000;
/// Cap for the final context string passed into the graph.
const MAX_CONTEXT_CHARS: usize = 14_000;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextFile {
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summarized_through_id: Option<String>,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub message_count_summarized: u32,
}

#[derive(Clone, Debug)]
pub struct ConversationContext {
    pub text: String,
    pub compressed: bool,
    pub summary_chars: usize,
    pub recent_chars: usize,
}

#[derive(Clone)]
pub struct CompressLlm {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub locale: String,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn context_path(chats_root: &Path, agent_id: &str, session_id: &str) -> std::path::PathBuf {
    chats_root.join(agent_id).join(session_id).join("context.json")
}

pub fn load_context_file(
    chats_root: &Path,
    agent_id: &str,
    session_id: &str,
) -> AppResult<SessionContextFile> {
    let path = context_path(chats_root, agent_id, session_id);
    if !path.is_file() {
        return Ok(SessionContextFile::default());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn save_context_file(
    chats_root: &Path,
    agent_id: &str,
    session_id: &str,
    file: &SessionContextFile,
) -> AppResult<()> {
    let path = context_path(chats_root, agent_id, session_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(file)?)?;
    Ok(())
}

fn is_context_worthy(entry: &TranscriptEntry) -> bool {
    match (entry.role.as_str(), entry.kind.as_str()) {
        ("user", "text") => !entry.content.trim().is_empty(),
        ("assistant", "markdown" | "error") => !entry.content.trim().is_empty(),
        ("assistant", "tool") => {
            // Keep a short breadcrumb of tools, not full SQL dumps.
            true
        }
        _ => false,
    }
}

fn format_entry(entry: &TranscriptEntry) -> String {
    match (entry.role.as_str(), entry.kind.as_str()) {
        ("user", _) => format!("User:\n{}", clip(&entry.content, 4_000)),
        ("assistant", "markdown") => format!("Assistant:\n{}", clip(&entry.content, 6_000)),
        ("assistant", "error") => format!("Assistant error:\n{}", clip(&entry.content, 1_500)),
        ("assistant", "tool") => {
            let tool = entry
                .meta
                .as_ref()
                .and_then(|m| m.get("tool"))
                .and_then(|v| v.as_str())
                .unwrap_or("tool");
            let ok = entry
                .meta
                .as_ref()
                .and_then(|m| m.get("ok"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            format!(
                "Tool {tool} ({}): {}",
                if ok { "ok" } else { "fail" },
                clip(&entry.content, 400)
            )
        }
        _ => format!("{}:\n{}", entry.role, clip(&entry.content, 1_000)),
    }
}

fn clip(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn char_len(value: &str) -> usize {
    value.chars().count()
}

/// Split entries into (older, recent) so recent stays under KEEP_RECENT_CHARS.
fn split_recent(entries: &[TranscriptEntry]) -> (Vec<TranscriptEntry>, Vec<TranscriptEntry>) {
    let mut recent_rev = Vec::new();
    let mut used = 0usize;
    for entry in entries.iter().rev() {
        let piece = format_entry(entry);
        let next = used + char_len(&piece) + 2;
        if !recent_rev.is_empty() && next > KEEP_RECENT_CHARS {
            break;
        }
        used = next;
        recent_rev.push(entry.clone());
    }
    recent_rev.reverse();
    let recent_ids: std::collections::HashSet<&str> =
        recent_rev.iter().map(|e| e.id.as_str()).collect();
    let older: Vec<TranscriptEntry> = entries
        .iter()
        .filter(|e| !recent_ids.contains(e.id.as_str()))
        .cloned()
        .collect();
    (older, recent_rev)
}

fn extractive_summary(entries: &[TranscriptEntry], locale: &str) -> String {
    let mut lines = Vec::new();
    for entry in entries {
        if entry.role == "user" {
            lines.push(format!("- Q: {}", clip(&entry.content, 240)));
        } else if entry.kind == "markdown" {
            let first = entry
                .content
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("");
            if !first.is_empty() {
                lines.push(format!("- A: {}", clip(first, 240)));
            }
        } else if entry.kind == "error" {
            lines.push(format!("- Error: {}", clip(&entry.content, 160)));
        }
    }
    let body = lines.join("\n");
    if locale == "zh" {
        format!("此前对话要点：\n{}", clip(&body, MAX_SUMMARY_CHARS))
    } else {
        format!("Prior conversation highlights:\n{}", clip(&body, MAX_SUMMARY_CHARS))
    }
}

async fn llm_summary(
    llm: &CompressLlm,
    previous_summary: &str,
    older: &[TranscriptEntry],
) -> AppResult<String> {
    let digest = older
        .iter()
        .map(format_entry)
        .collect::<Vec<_>>()
        .join("\n\n");
    let zh = llm.locale == "zh";
    let system = if zh {
        "你是对话历史压缩器。把旧消息压成简短、可继续分析用的摘要。保留：用户目标、已确认事实/数字、关键表与 SQL 结论、未解决问题。不要编造。"
    } else {
        "You compress chat history for a data analyst agent. Keep user goals, confirmed facts/numbers, key tables/SQL findings, and open questions. Do not invent."
    };
    let human = if zh {
        format!(
            "已有摘要：\n{}\n\n需要并入的旧消息：\n{}\n\n返回 JSON。",
            if previous_summary.trim().is_empty() {
                "（无）"
            } else {
                previous_summary
            },
            clip(&digest, 24_000)
        )
    } else {
        format!(
            "Existing summary:\n{}\n\nOlder messages to fold in:\n{}\n\nReturn JSON.",
            if previous_summary.trim().is_empty() {
                "(none)"
            } else {
                previous_summary
            },
            clip(&digest, 24_000)
        )
    };
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary"],
        "properties": {
            "summary": { "type": "string", "minLength": 1 }
        }
    });
    let result = complete(LlmCompleteRequest {
        provider: llm.provider.clone(),
        api_key: llm.api_key.clone(),
        model: llm.model.clone(),
        base_url: llm.base_url.clone(),
        system: system.into(),
        human,
        schema,
    })
    .await?;
    let summary = result
        .get("summary")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::msg("empty compression summary"))?;
    Ok(clip(summary, MAX_SUMMARY_CHARS))
}

fn entries_after_id<'a>(
    entries: &'a [TranscriptEntry],
    through_id: Option<&str>,
) -> &'a [TranscriptEntry] {
    let Some(through_id) = through_id.filter(|id| !id.is_empty()) else {
        return entries;
    };
    if let Some(idx) = entries.iter().position(|e| e.id == through_id) {
        let next = idx + 1;
        if next >= entries.len() {
            return &[];
        }
        return &entries[next..];
    }
    entries
}

/// Build prompt context for a session: rolling summary + recent messages.
/// Compresses older history when the conversation is long.
pub async fn prepare_conversation_context(
    chats_root: &Path,
    agent_id: &str,
    session_id: &str,
    llm: Option<&CompressLlm>,
) -> AppResult<ConversationContext> {
    let detail = load_session(chats_root, agent_id, session_id)?;
    let worthy: Vec<TranscriptEntry> = detail
        .transcript
        .into_iter()
        .filter(|e| is_context_worthy(e))
        // Drop the trailing user turn — the live `query` is sent separately.
        .collect();
    let mut worthy = worthy;
    if let Some(last) = worthy.last() {
        if last.role == "user" && last.kind == "text" {
            worthy.pop();
        }
    }

    if worthy.is_empty() {
        return Ok(ConversationContext {
            text: String::new(),
            compressed: false,
            summary_chars: 0,
            recent_chars: 0,
        });
    }

    let mut stored = load_context_file(chats_root, agent_id, session_id)?;
    let (older_all, recent) = split_recent(&worthy);
    let unsummarized = entries_after_id(&older_all, stored.summarized_through_id.as_deref());

    let older_chars: usize = older_all.iter().map(|e| char_len(&format_entry(e)) + 2).sum();
    let recent_text = recent
        .iter()
        .map(format_entry)
        .collect::<Vec<_>>()
        .join("\n\n");
    let recent_chars = char_len(&recent_text);
    let needs_compress = !unsummarized.is_empty()
        && (older_chars + recent_chars + char_len(&stored.summary) >= COMPRESS_TRIGGER_CHARS
            || stored.summary.trim().is_empty() && older_chars > KEEP_RECENT_CHARS / 2);

    let mut compressed = false;
    if needs_compress && !unsummarized.is_empty() {
        let locale = llm.map(|l| l.locale.as_str()).unwrap_or("en");
        let summary = if let Some(llm) = llm {
            match llm_summary(llm, &stored.summary, unsummarized).await {
                Ok(text) => text,
                Err(err) => {
                    log::warn!(
                        "conversation compress LLM failed, using extractive: {}",
                        err.user_message()
                    );
                    let merged = if stored.summary.trim().is_empty() {
                        extractive_summary(unsummarized, locale)
                    } else {
                        format!(
                            "{}\n{}",
                            stored.summary.trim(),
                            extractive_summary(unsummarized, locale)
                        )
                    };
                    clip(&merged, MAX_SUMMARY_CHARS)
                }
            }
        } else {
            let merged = if stored.summary.trim().is_empty() {
                extractive_summary(unsummarized, locale)
            } else {
                format!(
                    "{}\n{}",
                    stored.summary.trim(),
                    extractive_summary(unsummarized, locale)
                )
            };
            clip(&merged, MAX_SUMMARY_CHARS)
        };

        stored.summary = summary;
        stored.summarized_through_id = older_all.last().map(|e| e.id.clone());
        stored.message_count_summarized = older_all.len() as u32;
        stored.updated_at = now_iso();
        let _ = save_context_file(chats_root, agent_id, session_id, &stored);
        compressed = true;
    }

    let mut parts = Vec::new();
    if !stored.summary.trim().is_empty() {
        parts.push(stored.summary.trim().to_string());
    }
    if !recent_text.trim().is_empty() {
        parts.push(recent_text);
    }
    let text = clip(&parts.join("\n\n---\n\n"), MAX_CONTEXT_CHARS);

    Ok(ConversationContext {
        text,
        compressed,
        summary_chars: char_len(&stored.summary),
        recent_chars,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceCompressResult {
    pub context: SessionContextFile,
    pub compressed: bool,
    pub messages_compressed: u32,
}

/// Force compression of everything older than the recent window (manual UI action).
pub async fn force_compress_session(
    chats_root: &Path,
    agent_id: &str,
    session_id: &str,
    llm: Option<&CompressLlm>,
) -> AppResult<ForceCompressResult> {
    let detail = load_session(chats_root, agent_id, session_id)?;
    let worthy: Vec<TranscriptEntry> = detail
        .transcript
        .into_iter()
        .filter(|e| is_context_worthy(e))
        .collect();
    let (older, _recent) = split_recent(&worthy);
    let stored = load_context_file(chats_root, agent_id, session_id)?;
    if older.is_empty() {
        return Ok(ForceCompressResult {
            context: stored,
            compressed: false,
            messages_compressed: 0,
        });
    }
    let mut stored = stored;
    let locale = llm.map(|l| l.locale.as_str()).unwrap_or("en");
    let summary = if let Some(llm) = llm {
        llm_summary(llm, &stored.summary, &older)
            .await
            .unwrap_or_else(|_| extractive_summary(&older, locale))
    } else {
        extractive_summary(&older, locale)
    };
    let messages_compressed = older.len() as u32;
    stored.summary = summary;
    stored.summarized_through_id = older.last().map(|e| e.id.clone());
    stored.message_count_summarized = messages_compressed;
    stored.updated_at = now_iso();
    save_context_file(chats_root, agent_id, session_id, &stored)?;
    Ok(ForceCompressResult {
        context: stored,
        compressed: true,
        messages_compressed,
    })
}

pub fn context_event_payload(ctx: &ConversationContext) -> Value {
    json!({
        "type": "context",
        "compressed": ctx.compressed,
        "summaryChars": ctx.summary_chars,
        "recentChars": ctx.recent_chars,
        "totalChars": char_len(&ctx.text)
    })
}
