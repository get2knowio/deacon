#![allow(dead_code)]
#![allow(unused_imports)]

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Environment gate: set to `1` to enable parity tests.
const ENV_ENABLE: &str = "DEACON_PARITY";

/// Optional override for upstream read-configuration invocation.
/// Template placeholders: {config}, {workspace}
const ENV_UPSTREAM_READ_CONFIG_TEMPLATE: &str = "DEACON_PARITY_UPSTREAM_READ_CONFIGURATION";
/// Optional override for upstream devcontainer binary path.
const ENV_UPSTREAM_BIN: &str = "DEACON_PARITY_DEVCONTAINER";

pub fn repo_root() -> PathBuf {
    // crates/deacon -> repo root is two levels up
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(|p| p.parent())
        .unwrap_or(&here)
        .to_path_buf()
}

pub fn parity_enabled() -> bool {
    std::env::var(ENV_ENABLE).map(|v| v == "1").unwrap_or(false)
}

pub fn docker_available() -> bool {
    std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn upstream_available() -> bool {
    if !parity_enabled() {
        return false;
    }
    if let Ok(bin) = std::env::var(ENV_UPSTREAM_BIN) {
        return std::path::Path::new(&bin).exists();
    }
    std::process::Command::new("sh")
        .arg("-lc")
        .arg("command -v devcontainer >/dev/null 2>&1")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn skip_reason() -> String {
    if !parity_enabled() {
        return "DEACON_PARITY not set to 1".to_string();
    }
    if let Ok(bin) = std::env::var(ENV_UPSTREAM_BIN) {
        return format!("devcontainer CLI not found at {}", bin);
    }
    if !upstream_available() {
        return "`devcontainer` CLI not found in PATH".to_string();
    }
    String::from("unknown")
}

/// Run our CLI and return stdout as String on success.
pub fn run_deacon_read_configuration(config_path: &Path) -> anyhow::Result<String> {
    // Align workspace with upstream: use the directory containing the devcontainer folder
    // (i.e., parent of the config file's directory), falling back to repo_root if unknown.
    let workspace: std::path::PathBuf = config_path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(repo_root);

    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(&workspace)
        .arg("--config")
        .arg(config_path)
        .assert()
        .get_output()
        .to_owned();

    if !output.status.success() {
        anyhow::bail!(
            "deacon read-configuration failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Try to invoke the upstream CLI for read-configuration.
/// Attempts templates in order: env override, `--config`, `--workspace-folder`, positional path.
pub fn run_upstream_read_configuration(config_path: &Path) -> anyhow::Result<String> {
    let workspace = config_path
        .parent()
        .map(|p| p.parent().unwrap_or(p))
        .unwrap_or_else(|| Path::new("."));

    // 1) Env-provided template
    if let Ok(tpl) = std::env::var(ENV_UPSTREAM_READ_CONFIG_TEMPLATE) {
        if let Ok(s) = try_run_template(&tpl, config_path, workspace) {
            return Ok(s);
        }
    }

    // 2) Known attempt: --config <path>
    if let Ok(s) = run_devcontainer(
        &[
            "read-configuration",
            "--config",
            &config_path.to_string_lossy(),
        ],
        workspace,
    ) {
        return Ok(s);
    }

    // 3) Known attempt: --workspace-folder <workspace>
    if let Ok(s) = run_devcontainer(
        &[
            "read-configuration",
            "--workspace-folder",
            &workspace.to_string_lossy(),
        ],
        workspace,
    ) {
        return Ok(s);
    }

    // 4) Fallback: positional <workspace>
    if let Ok(s) = run_devcontainer(
        &["read-configuration", &workspace.to_string_lossy()],
        workspace,
    ) {
        return Ok(s);
    }

    anyhow::bail!("Unable to run upstream devcontainer read-configuration with known flag variants")
}

fn try_run_template(tpl: &str, config: &Path, workspace: &Path) -> anyhow::Result<String> {
    // Split a simple space-delimited template after substitution
    let cmdline = tpl
        .replace("{config}", &config.to_string_lossy())
        .replace("{workspace}", &workspace.to_string_lossy());
    let parts: Vec<String> = shell_words::split(&cmdline)?;
    run_devcontainer(&parts, workspace)
}

fn run_devcontainer<S: AsRef<str>>(args: &[S], cwd: &Path) -> anyhow::Result<String> {
    let bin = std::env::var(ENV_UPSTREAM_BIN).unwrap_or_else(|_| "devcontainer".to_string());
    let mut cmd = std::process::Command::new(bin);
    cmd.current_dir(cwd);
    for a in args {
        cmd.arg(a.as_ref());
    }
    let out = cmd.output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        anyhow::bail!(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Normalize JSON produced by both CLIs for functional comparison.
/// Drops volatile fields and returns a pruned object with core keys.
pub fn normalize_config_json(raw: &str) -> anyhow::Result<Value> {
    let v: Value = serde_json::from_str(raw.trim())?;
    // Upstream devcontainer CLI returns an object with a top-level "configuration" object.
    // Our CLI returns the effective configuration object directly. Handle both.
    let obj = match v.as_object() {
        Some(o) => o,
        None => return Ok(v),
    };
    if let Some(conf) = obj.get("configuration") {
        return Ok(extract_core_config(conf));
    }
    Ok(extract_core_config(&v))
}

/// Extract a subset of keys that define the effective devcontainer behavior.
fn extract_core_config(v: &Value) -> Value {
    use serde_json::{json, Map};
    let mut out = Map::new();
    let obj = match v.as_object() {
        Some(o) => o,
        None => return v.clone(),
    };

    for k in [
        "name",
        "image",
        "workspaceFolder",
        "dockerFile",
        "build",
        "features",
        "containerEnv",
        "mounts",
        "onCreateCommand",
        "postCreateCommand",
        "postStartCommand",
        "postAttachCommand",
    ] {
        if let Some(val) = obj.get(k) {
            // Skip nulls to avoid mismatches when one side omits null-valued keys
            if !val.is_null() {
                out.insert(k.to_string(), val.clone());
            }
        }
    }

    // customizations.vscode.extensions (if present)
    if let Some(customizations) = obj.get("customizations").and_then(|c| c.as_object()) {
        if let Some(vscode) = customizations.get("vscode").and_then(|c| c.as_object()) {
            if let Some(ext) = vscode.get("extensions").cloned() {
                out.insert(
                    "customizations".to_string(),
                    json!({ "vscode": { "extensions": ext } }),
                );
            }
        }
    }

    let mut core = Value::Object(out);
    sanitize_dynamic_values(&mut core);
    core
}

/// Write a devcontainer.json file to the .devcontainer directory
pub fn write_devcontainer(ws: &Path, json: &str) -> anyhow::Result<()> {
    let dc_dir = ws.join(".devcontainer");
    std::fs::create_dir_all(&dc_dir)?;
    std::fs::write(dc_dir.join("devcontainer.json"), json)?;
    Ok(())
}

/// Recursively sanitize dynamic IDs and placeholders so outputs are comparable.
fn sanitize_dynamic_values(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (_k, val) in map.iter_mut() {
                sanitize_dynamic_values(val);
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                sanitize_dynamic_values(val);
            }
        }
        Value::String(s) => {
            let mut replaced = s.replace("${devcontainerId}", "<ID>");
            replaced = replace_hex12(&replaced);
            *s = replaced;
        }
        _ => {}
    }
}

/// Replace any 12-character contiguous lowercase hex sequences with <ID>.
fn replace_hex12(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        // Attempt to match 12 hex chars starting at i
        if i + 12 <= bytes.len() {
            let slice = &bytes[i..i + 12];
            if is_hex_slice(slice) {
                out.push_str("<ID>");
                i += 12;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn is_hex_slice(slice: &[u8]) -> bool {
    slice.iter().all(|b| {
        matches!(b,
            b'0'..=b'9' | b'a'..=b'f'
        )
    })
}

/// Run upstream devcontainer command and return output
pub fn run_upstream(ws: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let bin = std::env::var(ENV_UPSTREAM_BIN).unwrap_or_else(|_| "devcontainer".to_string());
    let mut cmd = std::process::Command::new(bin);
    cmd.current_dir(ws);
    for arg in args {
        cmd.arg(arg);
    }
    Ok(cmd.output()?)
}

/// Run deacon command and return output
pub fn run_deacon(ws: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    use assert_cmd::Command;
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.current_dir(ws);
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.assert().get_output().to_owned();
    Ok(std::process::Output {
        status: output.status,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
    })
}

/// Extract stdout as trimmed string
pub fn stdout_str(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
