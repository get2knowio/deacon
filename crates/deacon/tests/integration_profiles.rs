//! Integration tests for user-scoped profiles (017).
//!
//! Exercises the `--profile` selection surface end-to-end via `read-configuration`
//! (config layering, fail-fast, three-state default) and `up` (the FR-020a
//! host-hook trust gate for an out-of-user-data fragment). Filesystem-only —
//! `read-configuration` needs no container, and the `up` trust test fails at the
//! workspace-trust gate before any container work. Contract scenarios C1–C11
//! from `specs/017-user-profiles/contracts/cli-profile.md`.

use assert_cmd::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// A workspace + a throwaway user-data folder (settings + fragments).
struct Fixture {
    workspace: TempDir,
    udf: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let workspace = TempDir::new().unwrap();
        let udf = TempDir::new().unwrap();
        Self { workspace, udf }
    }

    /// Write the workspace `.devcontainer/devcontainer.json`.
    fn workspace_config(&self, body: &str) -> &Self {
        let dir = self.workspace.path().join(".devcontainer");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("devcontainer.json"), body).unwrap();
        self
    }

    /// Write `{udf}/settings.json`.
    fn settings(&self, body: &str) -> &Self {
        fs::write(self.udf.path().join("settings.json"), body).unwrap();
        self
    }

    /// Write a fragment under the user-data folder (relative to settings dir).
    fn fragment(&self, rel: &str, body: &str) -> &Self {
        let path = self.udf.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
        self
    }

    fn ws(&self) -> &Path {
        self.workspace.path()
    }

    fn udf_path(&self) -> &Path {
        self.udf.path()
    }

    /// Run `read-configuration` with the given global-flag args (profile,
    /// override-config, …). Returns (success, parsed-stdout-or-null, stderr).
    fn read_config(&self, extra: &[&str]) -> (bool, Value, String) {
        let mut cmd = Command::cargo_bin("deacon").unwrap();
        cmd.arg("--user-data-folder")
            .arg(self.udf_path())
            .arg("--workspace-folder")
            .arg(self.ws());
        for a in extra {
            cmd.arg(a);
        }
        cmd.arg("read-configuration");
        let out = cmd.output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let json = serde_json::from_str(&stdout).unwrap_or(Value::Null);
        (out.status.success(), json, stderr)
    }
}

fn container_env<'a>(cfg: &'a Value, key: &str) -> Option<&'a str> {
    cfg["configuration"]["containerEnv"][key].as_str()
}

/// Two profiles referencing distinct fragments; a bare `alpine` base.
fn dev_agent_fixture() -> Fixture {
    let fx = Fixture::new();
    fx.workspace_config(r#"{ "name": "proj", "image": "alpine:3.18" }"#)
        .fragment(
            "overrides/dev.json",
            r#"{ "containerEnv": { "DEACON_MODE": "dev", "DEV_ONLY": "1" } }"#,
        )
        .fragment(
            "overrides/agent.json",
            r#"{ "containerEnv": { "DEACON_MODE": "agent" } }"#,
        )
        .settings(
            r#"{
                "defaultProfile": "dev",
                "profiles": {
                    "dev":   { "mergeConfig": "overrides/dev.json" },
                    "agent": { "mergeConfig": "overrides/agent.json", "browser": "none" }
                }
            }"#,
        );
    fx
}

// ---- User Story 1 (T007): C1, C5, C8, C9 ----

#[test]
fn c1_selected_agent_applies_only_agent_override() {
    let fx = dev_agent_fixture();
    let (ok, cfg, stderr) = fx.read_config(&["--profile", "agent"]);
    assert!(ok, "read-configuration failed: {stderr}");
    assert_eq!(container_env(&cfg, "DEACON_MODE"), Some("agent"));
    // The dev-only override must NOT bleed in.
    assert!(
        cfg["configuration"]["containerEnv"]
            .get("DEV_ONLY")
            .is_none(),
        "unselected dev override leaked: {}",
        cfg["configuration"]["containerEnv"]
    );
}

#[test]
fn c1_selected_dev_applies_only_dev_override() {
    let fx = dev_agent_fixture();
    let (ok, cfg, stderr) = fx.read_config(&["--profile", "dev"]);
    assert!(ok, "{stderr}");
    assert_eq!(container_env(&cfg, "DEACON_MODE"), Some("dev"));
    assert_eq!(container_env(&cfg, "DEV_ONLY"), Some("1"));
}

#[test]
fn c5_unknown_profile_errors_listing_available() {
    let fx = dev_agent_fixture();
    let (ok, _cfg, stderr) = fx.read_config(&["--profile", "nope"]);
    assert!(!ok, "expected non-zero exit for unknown profile");
    assert!(stderr.contains("nope"), "stderr: {stderr}");
    assert!(
        stderr.contains("dev, agent"),
        "should list available in declaration order: {stderr}"
    );
}

#[test]
fn c5_unknown_profile_via_env_var_errors() {
    let fx = dev_agent_fixture();
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.env("DEACON_PROFILE", "nope")
        .arg("--user-data-folder")
        .arg(fx.udf_path())
        .arg("--workspace-folder")
        .arg(fx.ws())
        .arg("read-configuration");
    let out = cmd.output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("nope") && stderr.contains("dev, agent"),
        "{stderr}"
    );
}

#[test]
fn c8_ordered_list_later_wins_and_sits_below_cli_merge() {
    let fx = Fixture::new();
    let cli_merge = fx.workspace.path().join("cli.json");
    fs::write(&cli_merge, r#"{ "containerEnv": { "K": "cli" } }"#).unwrap();
    fx.workspace_config(r#"{ "name": "proj", "image": "alpine:3.18" }"#)
        .fragment("overrides/a.json", r#"{ "containerEnv": { "K": "a" } }"#)
        .fragment("overrides/b.json", r#"{ "containerEnv": { "K": "b" } }"#)
        .settings(
            r#"{ "profiles": { "p": { "mergeConfig": ["overrides/a.json", "overrides/b.json"] } } }"#,
        );

    // Within the profile list, later (b) wins over earlier (a).
    let (ok, cfg, stderr) = fx.read_config(&["--profile", "p"]);
    assert!(ok, "{stderr}");
    assert_eq!(container_env(&cfg, "K"), Some("b"));

    // The CLI --merge-config is the highest merge layer, above the profile chain.
    let (ok, cfg, stderr) = fx.read_config(&[
        "--profile",
        "p",
        "--merge-config",
        cli_merge.to_str().unwrap(),
    ]);
    assert!(ok, "{stderr}");
    assert_eq!(container_env(&cfg, "K"), Some("cli"));
}

/// Reference parity (#285): `--override-config` REPLACES the discovered base —
/// the workspace `devcontainer.json` fields do not survive. Distinct from
/// `--merge-config`, which layers on top (see [`c8_ordered_list_later_wins_and_sits_below_cli_merge`]).
#[test]
fn override_config_replaces_discovered_base_via_cli() {
    let fx = Fixture::new();
    let replacement = fx.workspace.path().join("replacement.json");
    fs::write(
        &replacement,
        r#"{ "name": "replaced", "image": "debian:bookworm-slim" }"#,
    )
    .unwrap();
    // The discovered base sets a distinctive field the replacement omits.
    fx.workspace_config(
        r#"{ "name": "discovered", "image": "alpine:3.18", "containerEnv": { "FROM_BASE": "1" } }"#,
    );

    let (ok, cfg, stderr) = fx.read_config(&["--override-config", replacement.to_str().unwrap()]);
    assert!(ok, "{stderr}");
    assert_eq!(cfg["configuration"]["name"], "replaced");
    // The discovered base's containerEnv must be GONE — replace, not merge.
    assert!(
        cfg["configuration"]["containerEnv"]
            .as_object()
            .is_none_or(|m| m.is_empty()),
        "discovered base leaked through --override-config: {}",
        cfg["configuration"]["containerEnv"]
    );
}

#[test]
fn c9_empty_profile_applies_nothing() {
    let fx = Fixture::new();
    fx.workspace_config(r#"{ "name": "proj", "image": "alpine:3.18" }"#)
        .settings(r#"{ "profiles": { "vanilla": {} } }"#);
    let (ok, cfg, stderr) = fx.read_config(&["--profile", "vanilla"]);
    assert!(ok, "{stderr}");
    assert_eq!(cfg["configuration"]["name"], "proj");
    assert!(
        cfg["configuration"]["containerEnv"]
            .as_object()
            .is_none_or(|m| m.is_empty())
    );
}

// ---- User Story 1 defense-in-depth (T030): C11 ----

#[test]
fn c11_workspace_profiles_key_is_not_a_profile_source() {
    let fx = Fixture::new();
    // The workspace config carries its own `profiles`/`defaultProfile` keys —
    // these are ordinary unknown config fields, never a profile source (FR-019).
    fx.workspace_config(
        r#"{
            "name": "proj",
            "image": "alpine:3.18",
            "profiles": { "wsonly": { "mergeConfig": "evil.json" } },
            "defaultProfile": "wsonly"
        }"#,
    )
    .settings(r#"{ "profiles": { "real": {} } }"#);

    // A profile named only in the workspace is unknown — profiles come from the
    // user-data folder only; the error lists the settings profiles, not `wsonly`.
    let (ok, _cfg, stderr) = fx.read_config(&["--profile", "wsonly"]);
    assert!(!ok);
    assert!(stderr.contains("wsonly"), "{stderr}");
    assert!(
        stderr.contains("real") && !stderr.contains("wsonly,"),
        "{stderr}"
    );

    // A bare run applies nothing (the workspace `defaultProfile` is ignored) and
    // succeeds, echoing the workspace config as-is.
    let (ok, cfg, stderr) = fx.read_config(&[]);
    assert!(ok, "{stderr}");
    assert_eq!(cfg["configuration"]["name"], "proj");
}

// ---- User Story 2 (T016): C2, C3, C4, C6 ----

#[test]
fn c2_bare_run_applies_default_profile() {
    let fx = dev_agent_fixture();
    let (ok, cfg, stderr) = fx.read_config(&[]);
    assert!(ok, "{stderr}");
    assert_eq!(container_env(&cfg, "DEACON_MODE"), Some("dev"));
    assert_eq!(container_env(&cfg, "DEV_ONLY"), Some("1"));
}

#[test]
fn c2_explicit_profile_overrides_default() {
    let fx = dev_agent_fixture();
    let (ok, cfg, stderr) = fx.read_config(&["--profile", "agent"]);
    assert!(ok, "{stderr}");
    assert_eq!(container_env(&cfg, "DEACON_MODE"), Some("agent"));
}

#[test]
fn c3_profiles_without_default_apply_nothing() {
    let fx = Fixture::new();
    fx.workspace_config(r#"{ "name": "proj", "image": "alpine:3.18" }"#)
        .fragment(
            "overrides/dev.json",
            r#"{ "containerEnv": { "DEACON_MODE": "dev" } }"#,
        )
        .settings(r#"{ "profiles": { "dev": { "mergeConfig": "overrides/dev.json" } } }"#);
    let (ok, cfg, stderr) = fx.read_config(&[]);
    assert!(ok, "{stderr}");
    assert_eq!(cfg["configuration"]["name"], "proj");
    assert!(
        cfg["configuration"]["containerEnv"]
            .get("DEACON_MODE")
            .is_none()
    );
}

#[test]
fn c4_no_profiles_key_is_unchanged() {
    let fx = Fixture::new();
    fx.workspace_config(r#"{ "name": "proj", "image": "alpine:3.18" }"#)
        .settings(r#"{ "browser": "firefox" }"#);
    let (ok, cfg, stderr) = fx.read_config(&[]);
    assert!(ok, "{stderr}");
    assert_eq!(cfg["configuration"]["name"], "proj");
    assert_eq!(cfg["configuration"]["image"], "alpine:3.18");
}

#[test]
fn c6_dangling_default_profile_errors_at_load() {
    let fx = Fixture::new();
    fx.workspace_config(r#"{ "name": "proj", "image": "alpine:3.18" }"#)
        .settings(r#"{ "defaultProfile": "typo", "profiles": { "dev": {}, "agent": {} } }"#);
    let (ok, _cfg, stderr) = fx.read_config(&[]);
    assert!(!ok, "a dangling defaultProfile must fail fast");
    assert!(
        stderr.contains("typo") && stderr.contains("dev, agent"),
        "{stderr}"
    );
}

// ---- Missing fragment (FR-017) ----

#[test]
fn missing_fragment_names_owning_profile() {
    let fx = Fixture::new();
    fx.workspace_config(r#"{ "name": "proj", "image": "alpine:3.18" }"#)
        .settings(r#"{ "profiles": { "dev": { "mergeConfig": "overrides/gone.json" } } }"#);
    let (ok, _cfg, stderr) = fx.read_config(&["--profile", "dev"]);
    assert!(!ok);
    assert!(
        stderr.contains("dev") && stderr.contains("gone.json"),
        "{stderr}"
    );
}

// ---- Per-subcommand parity: build honors --profile the same way ----

#[test]
fn build_honors_unknown_profile_fail_fast() {
    let fx = dev_agent_fixture();
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("--user-data-folder")
        .arg(fx.udf_path())
        .arg("--workspace-folder")
        .arg(fx.ws())
        .arg("--profile")
        .arg("nope")
        .arg("build");
    let out = cmd.output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("nope") && stderr.contains("dev, agent"),
        "{stderr}"
    );
}

// ---- Per-subcommand parity: outdated layers the profile fragment (T015) ----

#[test]
fn outdated_layers_profile_override_fragment_feature() {
    let fx = Fixture::new();
    // Base project declares one feature; the profile fragment adds another.
    fx.workspace_config(r#"{ "features": { "ghcr.io/devcontainers/features/rust:1": {} } }"#)
        .fragment(
            "overrides/node.json",
            r#"{ "features": { "ghcr.io/devcontainers/features/node:18": {} } }"#,
        )
        .settings(r#"{ "profiles": { "p": { "mergeConfig": "overrides/node.json" } } }"#);

    // Force OCI failure (no network) — the feature keys still print, proving the
    // profile fragment layered into outdated's resolved config (FR-011/FR-014).
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.env(
        "DEACON_CUSTOM_CA_BUNDLE",
        fx.workspace.path().join("nonexistent-ca.pem"),
    )
    .arg("--user-data-folder")
    .arg(fx.udf_path())
    .arg("--profile")
    .arg("p")
    .arg("outdated")
    .arg("--workspace-folder")
    .arg(fx.ws());
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "outdated failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ghcr.io/devcontainers/features/node"),
        "profile feature missing: {stdout}"
    );
    // The base project feature is still present (merge, not replace).
    assert!(
        stdout.contains("ghcr.io/devcontainers/features/rust"),
        "base feature missing: {stdout}"
    );
}

// ---- User Story 3 (T020): C10 — env beats the profile browser value ----
//
// The browser resolver (`deacon_core::browser::resolve_browser`) applies the
// FR-013 precedence `env > profile/root value`; the effective profile value is
// produced by `Settings::resolve`. Both are unit-tested in-crate
// (`core::browser` and `core::settings`), which is the reader-level test T020
// permits. This integration check confirms the two compose to the FR-013 order.

#[test]
fn c10_env_beats_profile_browser_value() {
    use deacon_core::browser::{ResolvedBrowser, resolve_browser};
    use deacon_core::settings::Settings;

    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("settings.json"),
        r#"{ "browser": "firefox", "profiles": { "agent": { "browser": "chromium" } } }"#,
    )
    .unwrap();
    let settings = Settings::load(Some(tmp.path())).unwrap();
    let effective = settings.resolve(Some("agent"), tmp.path()).unwrap().browser;
    assert_eq!(effective.as_deref(), Some("chromium"));

    // DEACON_BROWSER wins over the profile-resolved value.
    assert_eq!(
        resolve_browser(Some("opera"), effective.as_deref()),
        ResolvedBrowser::Program("opera".to_string())
    );
    // Absent env ⇒ the profile value is used.
    assert_eq!(
        resolve_browser(None, effective.as_deref()),
        ResolvedBrowser::Program("chromium".to_string())
    );
}

// ---- T028: FR-020a host-hook trust gate for an out-of-user-data fragment ----
//
// A profile fragment referenced by an ABSOLUTE path OUTSIDE the user-data folder
// is NOT owner-guaranteed, so an `initializeCommand` it introduces stays subject
// to the workspace-trust gate. On an untrusted workspace the host hook must be
// refused before it runs. (The owner-authored bypass branch is covered
// hermetically by the in-crate `execute_initialize_command` unit tests.)
//
// This exercises `up` but never reaches container creation — the gate denies
// first — so it needs no Docker daemon.
#[cfg(unix)]
#[test]
fn t028_outside_user_data_fragment_initialize_command_is_gated() {
    let fx = Fixture::new();
    let outside = TempDir::new().unwrap();
    let marker = outside.path().join("ran.marker");
    let outside_fragment = outside.path().join("repo-fragment.json");
    fs::write(
        &outside_fragment,
        format!(r#"{{ "initializeCommand": "touch {}" }}"#, marker.display()),
    )
    .unwrap();

    fx.workspace_config(r#"{ "name": "proj", "image": "alpine:3.18" }"#)
        .settings(&format!(
            r#"{{ "profiles": {{ "outsider": {{ "mergeConfig": {:?} }} }} }}"#,
            outside_fragment
        ));

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.env("DEACON_NO_PROMPT", "1") // fail-closed: workspace is untrusted
        .arg("--user-data-folder")
        .arg(fx.udf_path())
        .arg("--workspace-folder")
        .arg(fx.ws())
        .arg("--profile")
        .arg("outsider")
        .arg("up");
    let out = cmd.output().unwrap();

    assert!(!out.status.success(), "gated up must fail");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.to_lowercase().contains("not trusted"),
        "expected a workspace-trust error, got: {combined}"
    );
    assert!(
        !marker.exists(),
        "the gated host hook must not have run from an out-of-user-data fragment"
    );
}
