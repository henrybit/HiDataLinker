use super::{AppError, AppResult, Grid, OracleSession};
use crate::models::ColumnMeta;
use async_trait::async_trait;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Connection settings for the official `oracle` crate (ODPI-C).
/// The password is kept for `Connection::connect` and is never written to logs.
#[derive(Clone)]
pub(super) struct OdpiSettings {
    username: String,
    password: String,
    client_lib_dir: Option<String>,
    pub(super) connect_string: String,
    pub(super) endpoint: String,
}

pub(super) fn settings(
    host: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
    database: Option<&str>,
    verify_cert: bool,
    oracle_connect: Option<&str>,
    client_lib_dir: Option<&str>,
) -> AppResult<OdpiSettings> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AppError::msg("host is required"));
    }
    let username = username.trim();
    if username.is_empty() {
        return Err(AppError::msg("username is required"));
    }
    let service = database.map(str::trim).unwrap_or("");
    let (use_sid, name) = super::oracle_target(service, oracle_connect)?;
    let port = if port == 0 { 1521 } else { port };
    let connect_string = connect_string(host, port, use_sid, name, verify_cert);
    let method = if use_sid {
        format!("sid={name}")
    } else {
        format!("service={name}")
    };
    let client_lib_dir = normalize_client_dir(client_lib_dir);
    let client_log = client_lib_dir.as_deref().unwrap_or("PATH");
    let endpoint = format!(
        "host={host} port={port} user={username} {method} tls={verify_cert} client={client_log} descriptor={connect_string}"
    );
    Ok(OdpiSettings {
        username: username.to_string(),
        password: password.unwrap_or("").to_string(),
        client_lib_dir,
        connect_string,
        endpoint,
    })
}

fn normalize_client_dir(dir: Option<&str>) -> Option<String> {
    let value = dir?.trim().trim_end_matches(['/', '\\']);
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(super) fn connect_string(
    host: &str,
    port: u16,
    use_sid: bool,
    name: &str,
    tls: bool,
) -> String {
    if use_sid || tls {
        let protocol = if tls { "TCPS" } else { "TCP" };
        let data = if use_sid {
            format!("(SID={name})")
        } else {
            format!("(SERVICE_NAME={name})")
        };
        format!(
            "(DESCRIPTION=(ADDRESS=(PROTOCOL={protocol})(HOST={host})(PORT={port}))(CONNECT_DATA={data}))"
        )
    } else {
        format!("{host}:{port}/{name}")
    }
}

pub(super) async fn open(settings: OdpiSettings) -> AppResult<Arc<dyn OracleSession>> {
    let endpoint = settings.endpoint.clone();
    log::info!("oracle connect via odpi-c: {endpoint}");
    let opened = tokio::task::spawn_blocking(move || connect_blocking(settings))
        .await
        .map_err(|error| AppError::msg(format!("oracle client worker failed: {error}")))?;
    match opened {
        Ok(session) => {
            log::info!("oracle odpi-c ok: {endpoint}");
            Ok(session)
        }
        Err(error) => {
            log::error!(
                "oracle odpi-c connect failed: {endpoint} | {}",
                error.user_message()
            );
            Err(error)
        }
    }
}

fn connect_blocking(settings: OdpiSettings) -> AppResult<Arc<dyn OracleSession>> {
    prepare_client(settings.client_lib_dir.as_deref())?;
    let connection = oracle::Connection::connect(
        settings.username,
        settings.password,
        settings.connect_string,
    )
    .map_err(map_odpi)?;
    Ok(Arc::new(OdpiSession {
        conn: Arc::new(connection),
    }))
}

struct OdpiSession {
    conn: Arc<oracle::Connection>,
}

#[async_trait]
impl OracleSession for OdpiSession {
    async fn ping(&self) -> AppResult<()> {
        let conn = Arc::clone(&self.conn);
        spawn(move || {
            conn.ping().map_err(map_odpi)?;
            Ok(())
        })
        .await
    }

    async fn query(&self, sql: &str, params: &[String]) -> AppResult<Grid> {
        let conn = Arc::clone(&self.conn);
        let sql = sql.to_string();
        let params = params.to_vec();
        spawn(move || query_blocking(&conn, &sql, &params)).await
    }

    async fn execute(&self, sql: &str) -> AppResult<u64> {
        let conn = Arc::clone(&self.conn);
        let sql = sql.to_string();
        spawn(move || {
            let statement = conn.execute(&sql, &[]).map_err(map_odpi)?;
            let count = statement.row_count().map_err(map_odpi)?;
            Ok(u64::from(count))
        })
        .await
    }

    async fn commit(&self) -> AppResult<()> {
        let conn = Arc::clone(&self.conn);
        spawn(move || {
            conn.commit().map_err(map_odpi)?;
            Ok(())
        })
        .await
    }

    async fn close(&self) -> AppResult<()> {
        let conn = Arc::clone(&self.conn);
        spawn(move || {
            conn.close().map_err(map_odpi)?;
            Ok(())
        })
        .await
    }
}

async fn spawn<T, F>(work: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| AppError::msg(format!("oracle client worker failed: {error}")))?
}

fn query_blocking(conn: &oracle::Connection, sql: &str, params: &[String]) -> AppResult<Grid> {
    let binds: Vec<&dyn oracle::sql_type::ToSql> = params.iter().map(|value| value as _).collect();
    let mut result = conn.query(sql, &binds).map_err(map_odpi)?;
    let columns = result
        .column_info()
        .iter()
        .map(|info| ColumnMeta {
            name: info.name().to_string(),
            type_name: format!("{:?}", info.oracle_type()),
        })
        .collect::<Vec<_>>();
    let width = columns.len();
    let mut rows = Vec::new();
    let mut truncated = false;
    for row in result.by_ref() {
        if rows.len() >= super::MAX_RESULT_ROWS {
            truncated = true;
            break;
        }
        let row = row.map_err(map_odpi)?;
        let mut cells = Vec::with_capacity(width);
        for index in 0..width {
            cells.push(odpi_cell(&row, index));
        }
        rows.push(cells);
    }
    Ok(Grid {
        columns,
        rows,
        affected_rows: 0,
        truncated,
    })
}

fn odpi_cell(row: &oracle::Row, index: usize) -> Option<String> {
    if let Ok(value) = row.get::<_, Option<String>>(index) {
        return value;
    }
    if let Ok(value) = row.get::<_, Option<i64>>(index) {
        return value.map(|number| number.to_string());
    }
    if let Ok(value) = row.get::<_, Option<f64>>(index) {
        return value.map(|number| number.to_string());
    }
    if let Ok(value) = row.get::<_, Option<oracle::sql_type::Timestamp>>(index) {
        return value.map(|timestamp| timestamp.to_string());
    }
    if let Ok(Some(bytes)) = row.get::<_, Option<Vec<u8>>>(index) {
        return Some(super::to_hex(&bytes));
    }
    None
}

/// Directory already passed to ODPI-C. `None` means the process searched PATH.
static LOADED_CLIENT: Mutex<Option<Option<String>>> = Mutex::new(None);

fn prepare_client(dir: Option<&str>) -> AppResult<()> {
    let requested = normalize_client_dir(dir);
    if let Some(path) = &requested {
        validate_client_dir(path)?;
    }
    let mut loaded = LOADED_CLIENT.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(current) = loaded.as_ref() {
        if current == &requested || requested.is_none() {
            return Ok(());
        }
        return Err(AppError::msg(format!(
            "Oracle Instant Client is already loaded from {}. Restart the app before using {}.",
            current.clone().unwrap_or_else(|| "PATH".to_string()),
            requested.unwrap_or_else(|| "PATH".to_string())
        )));
    }
    if let Some(path) = requested.clone() {
        let applied = oracle::InitParams::new()
            .oracle_client_lib_dir(&path)
            .map_err(|error| AppError::msg(error.to_string()))?
            .init()
            .map_err(map_odpi)?;
        if !applied {
            return Err(AppError::msg(
                "Oracle Instant Client was already loaded from PATH. Restart the app before using a different Instant Client directory.",
            ));
        }
        log::info!("oracle ODPI-C loaded Instant Client from {path}");
        *loaded = Some(Some(path));
        return Ok(());
    }
    if oracle::InitParams::is_initialized() {
        *loaded = Some(None);
    }
    Ok(())
}

fn client_library_name() -> &'static str {
    if cfg!(windows) {
        "oci.dll"
    } else if cfg!(target_os = "macos") {
        "libclntsh.dylib"
    } else {
        "libclntsh.so"
    }
}

fn validate_client_dir(path: &str) -> AppResult<()> {
    let dir = Path::new(path);
    let library = client_library_name();
    if !dir.is_dir() {
        return Err(AppError::msg(format!(
            "Oracle Instant Client directory not found: {path}. Choose the unzipped folder that contains {library} directly, for example C:\\oracle\\instantclient_19_22."
        )));
    }
    let mut found = dir.join(library).is_file();
    if !found && !cfg!(windows) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            found = entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("libclntsh.so")
            });
        }
    }
    if !found {
        return Err(AppError::msg(format!(
            "No {library} in {path}. Point this at the Instant Client folder that contains {library} directly, for example C:\\oracle\\instantclient_19_22. Do not choose the sdk or network\\admin folder."
        )));
    }
    Ok(())
}

fn map_odpi(error: oracle::Error) -> AppError {
    let app = AppError::Odpi(error);
    let text = app.user_message();
    if text.contains("DPI-1047") {
        AppError::msg(format!(
            "{text}. Set Instant Client directory to the unzipped folder that contains oci.dll, for example C:\\oracle\\instantclient_19_22, or add that folder to PATH."
        ))
    } else {
        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_uses_easy_connect() {
        assert_eq!(
            connect_string("db.example", 1521, false, "ORCL", false),
            "db.example:1521/ORCL"
        );
    }

    #[test]
    fn sid_and_tls_use_a_descriptor_without_the_password() {
        let settings = settings(
            "172.19.3.11",
            7026,
            "system",
            Some("s3cret-password"),
            Some("DB11G"),
            false,
            Some("sid"),
            None,
        )
        .unwrap();
        assert!(settings.connect_string.contains("(SID=DB11G)"));
        assert!(settings.connect_string.contains("(PROTOCOL=TCP)"));
        assert!(!settings.connect_string.contains("s3cret-password"));
        assert!(!settings.endpoint.contains("s3cret-password"));

        let tls = connect_string("db.example", 2484, false, "ORCL", true);
        assert!(tls.contains("(PROTOCOL=TCPS)"));
        assert!(tls.contains("(SERVICE_NAME=ORCL)"));
    }

    #[test]
    fn client_dir_is_logged_without_the_password() {
        let settings = settings(
            "172.19.3.11",
            1521,
            "system",
            Some("s3cret-password"),
            Some("DB11G"),
            false,
            Some("sid"),
            Some(r"C:\oracle\instantclient_19_22\"),
        )
        .unwrap();
        assert_eq!(
            settings.client_lib_dir.as_deref(),
            Some(r"C:\oracle\instantclient_19_22")
        );
        assert!(settings
            .endpoint
            .contains(r"client=C:\oracle\instantclient_19_22"));
        assert!(!settings.endpoint.contains("s3cret-password"));
    }

    #[test]
    fn client_dir_must_contain_the_library() {
        let missing = std::env::temp_dir().join(format!("db-gui-missing-oci-{}", std::process::id()));
        let error = validate_client_dir(&missing.to_string_lossy()).unwrap_err();
        let text = error.to_string();
        assert!(text.contains(client_library_name()));
        assert!(text.contains("instantclient_19_22"));

        let dir = std::env::temp_dir().join(format!("db-gui-oci-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let empty = validate_client_dir(&dir.to_string_lossy()).unwrap_err();
        assert!(empty.to_string().contains("sdk"));
        std::fs::write(dir.join(client_library_name()), b"").unwrap();
        assert!(validate_client_dir(&dir.to_string_lossy()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
