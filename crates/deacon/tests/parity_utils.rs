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
    let mut cmd = Command::cargo_bin("deacon")?;
    let output = cmd
        .arg("read-configuration")
        .arg("--workspace-folder")
        .arg(repo_root())
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
        if let Some(val) = obj.get(k).cloned() {
            out.insert(k.to_string(), val);
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

    Value::Object(out)
}

/// Write a devcontainer.json file to the .devcontainer directory
pub fn write_devcontainer(ws: &Path, json: &str) -> anyhow::Result<()> {
    let dc_dir = ws.join(".devcontainer");
    std::fs::create_dir_all(&dc_dir)?;
    std::fs::write(dc_dir.join("devcontainer.json"), json)?;
    Ok(())
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
