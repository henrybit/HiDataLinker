//! Agent-authored scripts under `~/.hidatalinker/temp/scripts/<runId>/`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::process::Command;

use super::policy::SqlAuthMode;

const MAX_SCRIPT_BYTES: usize = 512 * 1024;
const MAX_OUTPUT_CHARS: usize = 64 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 60_000;
const MAX_TIMEOUT_MS: u64 = 120_000;

type ScriptResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptRuntime {
    Python,
    Node,
    Shell,
}

impl ScriptRuntime {
    fn from_filename(name: &str) -> ScriptResult<Self> {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".py") {
            Ok(Self::Python)
        } else if lower.ends_with(".js") || lower.ends_with(".mjs") || lower.ends_with(".cjs") {
            Ok(Self::Node)
        } else if lower.ends_with(".sh") {
            Ok(Self::Shell)
        } else {
            Err("filename must end with .py, .js/.mjs/.cjs, or .sh".into())
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Node => "node",
            Self::Shell => "shell",
        }
    }
}

pub fn is_script_tool(name: &str) -> bool {
    matches!(name, "write_script" | "run_script" | "list_scripts")
}

pub fn require_full_auth(auth_mode: SqlAuthMode) -> ScriptResult<()> {
    if auth_mode != SqlAuthMode::Full {
        return Err("script tools require full authorization mode".into());
    }
    Ok(())
}

pub fn require_run_id(run_id: Option<&str>) -> ScriptResult<String> {
    let id = run_id
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            "script tools are only available inside a desktop agent run".to_string()
        })?;
    sanitize_run_id(id)
}

fn sanitize_run_id(run_id: &str) -> ScriptResult<String> {
    if run_id.len() > 80
        || !run_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err("invalid run id".into());
    }
    Ok(run_id.to_string())
}

pub fn sanitize_filename(name: &str) -> ScriptResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("filename is required".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("filename must be a plain name without path separators".into());
    }
    if name.len() > 120
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
    {
        return Err("filename may only contain letters, digits, '.', '-', '_'".into());
    }
    let _ = ScriptRuntime::from_filename(name)?;
    Ok(name.to_string())
}

pub fn workspace_dir(temp_root: &Path, run_id: &str) -> ScriptResult<PathBuf> {
    let run_id = sanitize_run_id(run_id)?;
    let dir = temp_root.join("scripts").join(run_id);
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create script workspace: {error}"))?;
    Ok(dir)
}

fn resolve_in_workspace(workspace: &Path, filename: &str) -> ScriptResult<PathBuf> {
    let filename = sanitize_filename(filename)?;
    let path = workspace.join(&filename);
    let workspace_canon = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let parent = path.parent().unwrap_or(workspace);
    let parent_canon = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    if !parent_canon.starts_with(&workspace_canon) {
        return Err("script path escapes the temp workspace".into());
    }
    Ok(path)
}

pub fn write_script(
    temp_root: &Path,
    run_id: &str,
    filename: &str,
    content: &str,
    overwrite: bool,
) -> ScriptResult<Value> {
    if content.len() > MAX_SCRIPT_BYTES {
        return Err(format!("script content exceeds {MAX_SCRIPT_BYTES} bytes"));
    }
    if content.contains('\0') {
        return Err("script content must not contain null bytes".into());
    }
    let workspace = workspace_dir(temp_root, run_id)?;
    let path = resolve_in_workspace(&workspace, filename)?;
    let runtime = ScriptRuntime::from_filename(&sanitize_filename(filename)?)?;
    if path.exists() && !overwrite {
        return Err(format!(
            "script already exists: {}; pass overwrite=true to replace",
            path.file_name().and_then(|s| s.to_str()).unwrap_or("?")
        ));
    }
    std::fs::write(&path, content)
        .map_err(|error| format!("failed to write script: {error}"))?;
    #[cfg(unix)]
    if runtime == ScriptRuntime::Shell {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(&path, perms);
    }
    Ok(json!({
        "ok": true,
        "filename": path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
        "path": path.display().to_string(),
        "runtime": runtime.label(),
        "bytes": content.len(),
        "workspace": workspace.display().to_string(),
    }))
}

pub fn list_scripts(temp_root: &Path, run_id: &str) -> ScriptResult<Value> {
    let workspace = workspace_dir(temp_root, run_id)?;
    let mut items = Vec::new();
    let entries = std::fs::read_dir(&workspace)
        .map_err(|error| format!("failed to list scripts: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if sanitize_filename(name).is_err() {
            continue;
        }
        let meta = entry.metadata().ok();
        items.push(json!({
            "filename": name,
            "runtime": ScriptRuntime::from_filename(name).map(|r| r.label()).unwrap_or("unknown"),
            "bytes": meta.map(|m| m.len()).unwrap_or(0),
        }));
    }
    items.sort_by(|a, b| {
        a.get("filename")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(b.get("filename").and_then(Value::as_str).unwrap_or(""))
    });
    Ok(json!({
        "workspace": workspace.display().to_string(),
        "scripts": items,
    }))
}

pub async fn run_script(
    temp_root: &Path,
    run_id: &str,
    filename: &str,
    args: &[String],
    timeout_ms: Option<u64>,
) -> ScriptResult<Value> {
    let workspace = workspace_dir(temp_root, run_id)?;
    let path = resolve_in_workspace(&workspace, filename)?;
    if !path.is_file() {
        return Err(format!(
            "script not found: {}",
            sanitize_filename(filename)?
        ));
    }
    let runtime = ScriptRuntime::from_filename(&sanitize_filename(filename)?)?;
    let timeout = timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(1_000, MAX_TIMEOUT_MS);

    let (program, mut cmd_args) = match runtime {
        ScriptRuntime::Python => {
            let bin = find_bin(&["python3", "python"])
                .ok_or_else(|| "python3/python not found on PATH".to_string())?;
            (bin, vec![path.display().to_string()])
        }
        ScriptRuntime::Node => {
            let bin =
                find_bin(&["node"]).ok_or_else(|| "node not found on PATH".to_string())?;
            (bin, vec![path.display().to_string()])
        }
        ScriptRuntime::Shell => {
            let bin = find_bin(&["bash", "sh"])
                .ok_or_else(|| "bash/sh not found on PATH".to_string())?;
            (bin, vec![path.display().to_string()])
        }
    };
    for arg in args {
        if arg.contains('\0') {
            return Err("script args must not contain null bytes".into());
        }
        if arg.len() > 4_096 {
            return Err("script arg is too long".into());
        }
        cmd_args.push(arg.clone());
    }

    let mut command = Command::new(&program);
    command
        .args(&cmd_args)
        .current_dir(&workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Keep a usable PATH for interpreters; strip HiDataLinker secrets.
    command.env_remove("HIDATALINKER_NODE");
    command.env_remove("HIDATALINKER_AGENTS_RUNTIME");

    let child = command
        .spawn()
        .map_err(|error| format!("failed to start script: {error}"))?;

    let output = match tokio::time::timeout(Duration::from_millis(timeout), child.wait_with_output())
        .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("script process error: {error}")),
        Err(_) => return Err(format!("script timed out after {timeout}ms")),
    };

    let stdout = truncate_output(&String::from_utf8_lossy(&output.stdout));
    let stderr = truncate_output(&String::from_utf8_lossy(&output.stderr));
    let exit_code = output.status.code();
    let ok = output.status.success();

    Ok(json!({
        "ok": ok,
        "filename": sanitize_filename(filename)?,
        "runtime": runtime.label(),
        "program": program.display().to_string(),
        "exitCode": exit_code,
        "timedOut": false,
        "timeoutMs": timeout,
        "stdout": stdout,
        "stderr": stderr,
        "workspace": workspace.display().to_string(),
    }))
}

fn truncate_output(text: &str) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= MAX_OUTPUT_CHARS {
            out.push_str("\n…[truncated]");
            break;
        }
        out.push(ch);
    }
    out
}

fn find_bin(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if is_runnable(&candidate) {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                let with_exe = dir.join(format!("{name}.exe"));
                if is_runnable(&with_exe) {
                    return Some(with_exe);
                }
                let with_cmd = dir.join(format!("{name}.cmd"));
                if is_runnable(&with_cmd) {
                    return Some(with_cmd);
                }
            }
        }
    }
    None
}

fn is_runnable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_filenames_and_detects_runtime() {
        assert_eq!(
            ScriptRuntime::from_filename("load.py").unwrap(),
            ScriptRuntime::Python
        );
        assert_eq!(
            ScriptRuntime::from_filename("load.js").unwrap(),
            ScriptRuntime::Node
        );
        assert_eq!(
            ScriptRuntime::from_filename("load.sh").unwrap(),
            ScriptRuntime::Shell
        );
        assert!(sanitize_filename("../x.py").is_err());
        assert!(sanitize_filename("a/b.py").is_err());
        assert!(sanitize_filename("ok_file-1.py").is_ok());
    }

    #[test]
    fn write_and_list_roundtrip() {
        let root = std::env::temp_dir().join(format!("hdl-script-{}", uuid::Uuid::new_v4()));
        let run_id = "run-abc";
        let written = write_script(
            &root,
            run_id,
            "hello.py",
            "print('hi')\n",
            false,
        )
        .unwrap();
        assert_eq!(written["filename"], "hello.py");
        assert_eq!(written["runtime"], "python");
        let listed = list_scripts(&root, run_id).unwrap();
        let scripts = listed["scripts"].as_array().unwrap();
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0]["filename"], "hello.py");
        assert!(write_script(&root, run_id, "hello.py", "x", false).is_err());
        assert!(write_script(&root, run_id, "hello.py", "print(2)\n", true).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn runs_shell_script_when_sh_available() {
        if find_bin(&["bash", "sh"]).is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!("hdl-script-run-{}", uuid::Uuid::new_v4()));
        let run_id = "run-shell";
        write_script(
            &root,
            run_id,
            "hello.sh",
            "#!/bin/sh\necho hello-from-shell\n",
            true,
        )
        .unwrap();
        let result = run_script(&root, run_id, "hello.sh", &[], Some(10_000))
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert!(result["stdout"].as_str().unwrap_or("").contains("hello-from-shell"));
        let _ = std::fs::remove_dir_all(root);
    }
}
