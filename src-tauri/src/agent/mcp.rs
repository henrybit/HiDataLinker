use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};

use super::catalog;

pub fn run() {
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout().lock();
    let mut framed = false;
    loop {
        match read_message(&mut reader, &mut framed) {
            Ok(None) => break,
            Ok(Some(message)) => {
                if let Some(response) = handle_message(&message) {
                    if write_message(&mut stdout, &response, framed).is_err() {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
}

fn handle_message(message: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    if id.is_none() {
        return None;
    }
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => Ok(initialize_result(message)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => tools_call(message.get("params").unwrap_or(&json!({}))),
        _ => Err((-32601, "method not found".to_string())),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        }),
    })
}

fn initialize_result(message: &Value) -> Value {
    let requested = message
        .pointer("/params/protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("2024-11-05");
    let version = if requested.starts_with("2024-") || requested.starts_with("2025-") {
        requested
    } else {
        "2024-11-05"
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "hidatalinker-db", "version": "1.0.0" },
        "instructions": "Use the running HiDataLinker desktop app. Do not ask for database passwords. query is read-only and accepts one statement."
    })
}

fn tool_list() -> Vec<Value> {
    catalog::tool_specs()
        .into_iter()
        .filter(|tool| !catalog::is_desktop_only_tool(tool.name))
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.schema,
            })
        })
        .collect()
}

fn tools_call(params: &Value) -> Result<Value, (i32, String)> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err((-32602, "tool name is required".into()));
    }
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let content = match post_tool(name, &arguments) {
        Ok(result) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()) }],
            "isError": false
        }),
        Err(message) => json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }),
    };
    Ok(content)
}

fn post_tool(name: &str, args: &Value) -> Result<Value, String> {
    let endpoint = read_endpoint()?;
    post_tool_at(endpoint.port, &endpoint.token, name, args)
}

fn read_endpoint() -> Result<Endpoint, String> {
    let home = crate::paths::home_dir().map_err(|error| error.user_message())?;
    let path = crate::paths::AppPaths::from_home(&home).agent_endpoint;
    let raw = std::fs::read_to_string(&path).map_err(|_| {
        "HiDataLinker is not running. Open the desktop app and enable external agents.".to_string()
    })?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| {
        "HiDataLinker agent endpoint is unreadable. Restart the desktop app.".to_string()
    })?;
    let port = value
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .ok_or_else(|| "HiDataLinker agent endpoint has no port".to_string())?;
    let token = value
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "HiDataLinker agent endpoint has no token".to_string())?
        .to_string();
    Ok(Endpoint { port, token })
}

struct Endpoint {
    port: u16,
    token: String,
}

fn post_tool_at(port: u16, token: &str, name: &str, args: &Value) -> Result<Value, String> {
    let body = serde_json::to_vec(args).map_err(|error| error.to_string())?;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(2)).map_err(|_| {
            "HiDataLinker is not running. Open the desktop app and enable external agents."
                .to_string()
        })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| error.to_string())?;
    let head = format!(
        "POST /v1/tools/{name} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|error| error.to_string())?;
    stream.write_all(&body).map_err(|error| error.to_string())?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|error| error.to_string())?;
    parse_http_json(&raw)
}

fn parse_http_json(raw: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| "response from HiDataLinker is not utf-8".to_string())?;
    let Some((_, body)) = text.split_once("\r\n\r\n") else {
        return Err("incomplete response from HiDataLinker".into());
    };
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    let payload: Value = serde_json::from_str(body.trim_end_matches('\0').trim())
        .map_err(|_| "HiDataLinker returned a non-JSON response".to_string())?;
    if status == 200 && payload.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(payload.get("result").cloned().unwrap_or(Value::Null));
    }
    Err(payload
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("HiDataLinker request failed")
        .to_string())
}

fn read_message(reader: &mut impl BufRead, framed: &mut bool) -> Result<Option<Value>, String> {
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('{') {
            let value = serde_json::from_str(trimmed).map_err(|error| error.to_string())?;
            return Ok(Some(value));
        }
        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            *framed = true;
            return Ok(Some(read_framed(reader, trimmed)?));
        }
        return Err("unrecognized mcp message".into());
    }
}

fn read_framed(reader: &mut impl BufRead, first: &str) -> Result<Value, String> {
    let mut length = content_length(first)?;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("unexpected end of mcp headers".into());
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            length = content_length(trimmed)?;
        }
    }
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

fn content_length(line: &str) -> Result<usize, String> {
    line.split_once(':')
        .and_then(|(_, value)| value.trim().parse().ok())
        .ok_or_else(|| "invalid content-length".to_string())
}

fn write_message(stdout: &mut impl Write, message: &Value, framed: bool) -> std::io::Result<()> {
    let body = serde_json::to_string(message)?;
    if framed {
        write!(stdout, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    } else {
        writeln!(stdout, "{body}")?;
    }
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn initialize_and_tool_list_do_not_need_the_app() {
        let response = handle_message(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-03-26" }
        }))
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(response["result"]["serverInfo"]["name"], "hidatalinker-db");

        let listed = handle_message(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }))
        .unwrap();
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert!(names.contains(&"list_connections"));
        assert!(names.contains(&"query"));
        assert!(handle_message(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .is_none());
    }

    #[test]
    fn reads_newline_and_framed_messages() {
        let mut framed = false;
        let mut reader = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
        let message = read_message(&mut reader, &mut framed).unwrap().unwrap();
        assert_eq!(message["method"], "ping");
        assert!(!framed);

        let body = br#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut raw = header.into_bytes();
        raw.extend_from_slice(body);
        let mut reader = Cursor::new(raw.as_slice());
        framed = false;
        let message = read_message(&mut reader, &mut framed).unwrap().unwrap();
        assert_eq!(message["id"], 7);
        assert!(framed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn proxy_forwards_a_read_and_rejects_a_write() {
        use super::super::http::handle_socket;
        use super::super::hub::AgentHub;
        use crate::models::ConnectionProfile;
        use crate::paths::AppPaths;
        use tokio::net::TcpListener;

        let root = std::env::temp_dir().join(format!("hidatalinker-mcp-{}", uuid::Uuid::new_v4()));
        let state = crate::state::AppState::load(AppPaths::from_root(root.clone()), None).unwrap();
        state
            .upsert_profile(ConnectionProfile {
                id: "conn-1".into(),
                name: "Orders".into(),
                engine: "postgres".into(),
                host: "127.0.0.1".into(),
                port: 5432,
                username: "ada".into(),
                password: Some("secret".into()),
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
        let hub = AgentHub::new("proxy-token".into(), true, root.join("agent.json"));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = std::sync::Arc::new(state);
        let hub = std::sync::Arc::new(hub);
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let state = state.clone();
                let hub = hub.clone();
                tokio::spawn(async move {
                    let _ = handle_socket(socket, &state, &hub).await;
                });
            }
        });

        let listed = tokio::task::spawn_blocking(move || {
            post_tool_at(port, "proxy-token", "list_connections", &json!({}))
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(listed[0]["name"], "Orders");
        assert!(!listed.to_string().contains("secret"));

        let rejected = tokio::task::spawn_blocking(move || {
            post_tool_at(
                port,
                "proxy-token",
                "query",
                &json!({"connectionId": "conn-1", "sql": "DROP TABLE orders"}),
            )
        })
        .await
        .unwrap()
        .unwrap_err();
        assert!(rejected.contains("DROP"), "{rejected}");
        let _ = std::fs::remove_dir_all(root);
    }
}
