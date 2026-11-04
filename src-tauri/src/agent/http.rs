use std::path::Path;
use std::time::Instant;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::catalog::{self, ToolError};
use super::hub::{AgentHub, AgentStatus, AuditEntry};
use crate::error::AppResult;
use crate::paths::AppPaths;
use crate::state::AppState;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;

struct Incoming {
    method: String,
    path: String,
    token: Option<String>,
    body: Vec<u8>,
}

pub(crate) struct Outgoing {
    status: u16,
    body: Value,
    audit: Option<AuditEntry>,
}

pub fn start(app: AppHandle, paths: &AppPaths) -> AppResult<()> {
    let enabled = AgentHub::load_enabled(&paths.agent_settings);
    let token = uuid::Uuid::new_v4().to_string();
    let hub = AgentHub::new(token, enabled, paths.agent_settings.clone());
    app.manage(hub);
    let endpoint = paths.agent_endpoint.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = listen(app, endpoint).await {
            eprintln!("agent gateway: {error}");
        }
    });
    Ok(())
}

async fn listen(app: AppHandle, endpoint: std::path::PathBuf) -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    {
        let hub = app.state::<AgentHub>();
        hub.set_port(port);
        write_endpoint(&endpoint, port, hub.token())?;
        let _ = app.emit("agent-gateway", hub.status());
    }
    loop {
        let (socket, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = serve_socket(socket, app).await {
                eprintln!("agent gateway request: {error}");
            }
        });
    }
}

async fn serve_socket(socket: TcpStream, app: AppHandle) -> Result<(), String> {
    let outcome = {
        let state = app.state::<AppState>();
        let hub = app.state::<AgentHub>();
        handle_socket(socket, &state, &hub).await?
    };
    if let Some(entry) = outcome.audit {
        let _ = app.emit("agent-audit", &entry);
        if entry.ok && entry.tool == "connect" {
            if let Some(connection_id) = entry.connection_id.clone() {
                let _ = app.emit(
                    "agent-session-changed",
                    json!({ "connectionId": connection_id }),
                );
            }
        }
    }
    Ok(())
}

pub(crate) async fn handle_socket(
    mut socket: TcpStream,
    state: &AppState,
    hub: &AgentHub,
) -> Result<Outgoing, String> {
    let incoming = match read_request(&mut socket).await {
        Ok(incoming) => incoming,
        Err(error) => {
            let outgoing = Outgoing {
                status: 400,
                body: json!({ "ok": false, "error": error }),
                audit: None,
            };
            write_response(&mut socket, &outgoing).await?;
            return Ok(outgoing);
        }
    };
    let outgoing = dispatch(state, hub, &incoming).await;
    write_response(&mut socket, &outgoing).await?;
    Ok(outgoing)
}

async fn dispatch(state: &AppState, hub: &AgentHub, incoming: &Incoming) -> Outgoing {
    if incoming.method == "GET" && incoming.path == "/v1/health" {
        return Outgoing {
            status: 200,
            body: json!({
                "ok": true,
                "enabled": hub.enabled(),
                "port": hub.port(),
            }),
            audit: None,
        };
    }

    let Some(tool) = incoming.path.strip_prefix("/v1/tools/") else {
        return Outgoing {
            status: 404,
            body: json!({ "ok": false, "error": "not found" }),
            audit: None,
        };
    };
    if tool.is_empty() || tool.contains('/') {
        return Outgoing {
            status: 404,
            body: json!({ "ok": false, "error": "not found" }),
            audit: None,
        };
    }
    if incoming.method != "POST" {
        return Outgoing {
            status: 405,
            body: json!({ "ok": false, "error": "method not allowed" }),
            audit: None,
        };
    }

    let started = Instant::now();
    let args: Value = if incoming.body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&incoming.body) {
            Ok(value) => value,
            Err(_) => {
                return finish(
                    hub,
                    tool,
                    &json!({}),
                    started,
                    400,
                    Err(ToolError::BadRequest("body must be JSON".into())),
                );
            }
        }
    };

    let authorized = incoming
        .token
        .as_deref()
        .is_some_and(|token| hub.tokens_match(token));
    if !authorized {
        return finish(
            hub,
            tool,
            &args,
            started,
            401,
            Err(ToolError::BadRequest("unauthorized".into())),
        );
    }
    if !hub.enabled() {
        return finish(
            hub,
            tool,
            &args,
            started,
            403,
            Err(ToolError::BadRequest(
                "MCP service is turned off in HiDataLinker settings".into(),
            )),
        );
    }
    if !catalog::is_tool(tool) {
        return finish(hub, tool, &args, started, 404, Err(ToolError::Unknown));
    }
    if catalog::is_desktop_only_tool(tool) {
        return finish(
            hub,
            tool,
            &args,
            started,
            403,
            Err(ToolError::BadRequest(
                "script tools are only available inside the desktop agent".into(),
            )),
        );
    }

    let result = catalog::invoke(state, tool, &args).await;
    let status = match &result {
        Ok(_) => 200,
        Err(ToolError::Unknown) => 404,
        Err(ToolError::BadRequest(_)) => 400,
        Err(ToolError::Failed(_)) => 500,
    };
    finish(hub, tool, &args, started, status, result)
}

fn finish(
    hub: &AgentHub,
    tool: &str,
    args: &Value,
    started: Instant,
    status: u16,
    result: Result<Value, ToolError>,
) -> Outgoing {
    let (connection_id, summary) = catalog::audit_fields(tool, args);
    let duration_ms = started.elapsed().as_millis() as u64;
    let (ok, error, body) = match result {
        Ok(result) => (true, None, json!({ "ok": true, "result": result })),
        Err(error) => {
            let message = error.message();
            (
                false,
                Some(message.clone()),
                json!({ "ok": false, "error": message }),
            )
        }
    };
    let audit = hub.record(
        tool.to_string(),
        connection_id,
        summary,
        ok,
        duration_ms,
        error,
    );
    Outgoing {
        status,
        body,
        audit: Some(audit),
    }
}

async fn read_request(socket: &mut TcpStream) -> Result<Incoming, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let header_end = loop {
        if buf.len() > MAX_HEADER_BYTES {
            return Err("headers too large".into());
        }
        let read = socket
            .read(&mut tmp)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed".into());
        }
        buf.extend_from_slice(&tmp[..read]);
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos;
        }
    };
    let header_text =
        std::str::from_utf8(&buf[..header_end]).map_err(|_| "headers are not utf-8")?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or("missing request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("")
        .to_string();
    if method.is_empty() || path.is_empty() {
        return Err("malformed request line".into());
    }
    let mut token = None;
    let mut content_length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value
                .trim()
                .parse()
                .map_err(|_| "invalid content-length".to_string())?;
        } else if name.eq_ignore_ascii_case("authorization") {
            let value = value.trim();
            token = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string);
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Err("body too large".into());
    }
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        let read = socket
            .read(&mut tmp)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("incomplete body".into());
        }
        body.extend_from_slice(&tmp[..read]);
        if body.len() > MAX_BODY_BYTES {
            return Err("body too large".into());
        }
    }
    body.truncate(content_length);
    Ok(Incoming {
        method,
        path,
        token,
        body,
    })
}

async fn write_response(socket: &mut TcpStream, outgoing: &Outgoing) -> Result<(), String> {
    let body = serde_json::to_vec(&outgoing.body).map_err(|error| error.to_string())?;
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        outgoing.status,
        reason(outgoing.status),
        body.len()
    );
    socket
        .write_all(head.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    socket
        .write_all(&body)
        .await
        .map_err(|error| error.to_string())?;
    let _ = socket.shutdown().await;
    Ok(())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub fn write_endpoint(path: &Path, port: u16, token: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let body = serde_json::to_string(&json!({ "port": port, "token": token }))
        .map_err(|error| error.to_string())?;
    let tmp = path.with_extension("json.tmp");
    write_secret(&tmp, &body)?;
    std::fs::rename(&tmp, path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_secret(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    file.write_all(contents.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn agent_status(hub: State<'_, AgentHub>) -> AgentStatus {
    hub.status()
}

#[tauri::command]
pub fn set_agent_enabled(
    app: AppHandle,
    hub: State<'_, AgentHub>,
    enabled: bool,
) -> Result<AgentStatus, String> {
    hub.set_enabled(enabled)?;
    let status = hub.status();
    let _ = app.emit("agent-gateway", &status);
    Ok(status)
}

#[tauri::command]
pub fn list_agent_audit(hub: State<'_, AgentHub>) -> Vec<AuditEntry> {
    hub.recent()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    async fn fixture() -> (AppState, AgentHub, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("hidatalinker-http-{}", uuid::Uuid::new_v4()));
        let paths = AppPaths::from_root(root.clone());
        let state = AppState::load(paths, None).unwrap();
        let hub = AgentHub::new(
            "token-1".into(),
            true,
            root.join("settings").join("agent.json"),
        );
        hub.set_port(9);
        (state, hub, root)
    }

    async fn exchange(port: u16, raw: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut text = Vec::new();
        let _ = stream.read_to_end(&mut text).await;
        String::from_utf8_lossy(&text).into_owned()
    }

    #[tokio::test]
    async fn gateway_auth_policy_and_redaction() {
        let (state, hub, root) = fixture().await;
        state
            .upsert_profile(crate::models::ConnectionProfile {
                id: "conn-1".into(),
                name: "Orders".into(),
                engine: "mysql".into(),
                host: "127.0.0.1".into(),
                port: 3306,
                username: "root".into(),
                password: Some("s3cret-password".into()),
                database: None,
                ssl_ca: None,
                ssl_cert: None,
                ssl_key: None,
                ssl_verify: false,
                oracle_version: None,
                oracle_connect: None,
                oracle_instant_client: None,
                save_password: true,
            })
            .await
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = std::sync::Arc::new(state);
        let hub = std::sync::Arc::new(hub);
        let state_task = state.clone();
        let hub_task = hub.clone();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let _ = handle_socket(socket, &state_task, &hub_task).await;
        });
        let health = exchange(port, "GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").await;
        assert!(health.contains("\"enabled\":true"), "{health}");

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state_task = state.clone();
        let hub_task = hub.clone();
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let state_task = state_task.clone();
                let hub_task = hub_task.clone();
                tokio::spawn(async move {
                    let _ = handle_socket(socket, &state_task, &hub_task).await;
                });
            }
        });

        let denied = exchange(
            port,
            "POST /v1/tools/list_connections HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{}",
        )
        .await;
        assert!(denied.starts_with("HTTP/1.1 401"), "{denied}");

        let body = "{}";
        let listed = exchange(
            port,
            &format!(
                "POST /v1/tools/list_connections HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer token-1\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            ),
        )
        .await;
        assert!(listed.contains("\"ok\":true"), "{listed}");
        assert!(!listed.contains("s3cret-password"), "{listed}");
        assert!(listed.contains("conn-1"), "{listed}");

        let sql = "{\"connectionId\":\"conn-1\",\"sql\":\"DELETE FROM orders\"}";
        let rejected = exchange(
            port,
            &format!(
                "POST /v1/tools/query HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer token-1\r\nContent-Length: {}\r\n\r\n{}",
                sql.len(),
                sql
            ),
        )
        .await;
        assert!(rejected.starts_with("HTTP/1.1 400"), "{rejected}");
        assert!(rejected.contains("DELETE"), "{rejected}");

        hub.set_enabled(false).unwrap();
        let disabled = exchange(
            port,
            &format!(
                "POST /v1/tools/list_connections HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer token-1\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            ),
        )
        .await;
        assert!(disabled.starts_with("HTTP/1.1 403"), "{disabled}");
        assert_eq!(hub.recent().len(), 4);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn endpoint_file_is_owner_readable() {
        let root =
            std::env::temp_dir().join(format!("hidatalinker-endpoint-{}", uuid::Uuid::new_v4()));
        let path = root.join("agent").join("endpoint.json");
        write_endpoint(&path, 4312, "token-1").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"port\":4312"));
        assert!(text.contains("token-1"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
