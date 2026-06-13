//! Host browser launching for port auto-open (`onAutoForward: openBrowser`).
//!
//! When the `--auto-forward` daemon forwards a port whose `onAutoForward`
//! attribute is `openBrowser`/`openBrowserOnce`, deacon opens the machine
//! owner's browser at the forwarded loopback URL. Which browser is a
//! **machine-owner** choice — `DEACON_BROWSER` env var, then the `browser` key
//! in `{user_data_folder}/settings.json`, then the OS default opener. It is
//! never sourced from the workspace (the workspace's `devcontainer.json` can
//! only *request* an open via `onAutoForward`; it can never choose the program).
//!
//! The browser value is a **bare program** (no shell): the URL is appended as
//! the final argument, so a hostile URL can't inject shell. Everything here is
//! best-effort — a missing/broken browser or headless host never fails `up` or
//! the daemon.

use crate::settings::Settings;
use std::process::Stdio;
use tracing::debug;

/// Env var the machine owner sets to choose the browser program.
/// Precedence: this > `settings.browser` > OS default.
pub const DEACON_BROWSER: &str = "DEACON_BROWSER";

/// Resolve the browser program from the machine-owner sources, in precedence
/// order: `DEACON_BROWSER` env > `settings.browser` > `None` (= OS default).
///
/// Empty/whitespace values are treated as unset at each tier (mirrors
/// [`crate::host_ca::resolve_host_ca_activation`]).
pub fn resolve_browser(env: Option<&str>, settings: &Settings) -> Option<String> {
    if let Some(v) = env {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    if let Some(v) = settings.browser.as_deref() {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// Build the `(program, args)` to open `url`. **Pure** (no spawn) so it is
/// unit-testable across platforms via `cfg!`.
///
/// - `Some(prog)` ⇒ run `prog <url>` (URL appended verbatim, no shell).
/// - `None` ⇒ the OS default opener: `open` (macOS), `cmd /C start "" <url>`
///   (Windows), `xdg-open` (Linux/other Unix).
pub fn browser_command(browser: Option<&str>, url: &str) -> (String, Vec<String>) {
    match browser {
        Some(prog) => (prog.to_string(), vec![url.to_string()]),
        None => {
            if cfg!(target_os = "macos") {
                ("open".to_string(), vec![url.to_string()])
            } else if cfg!(target_os = "windows") {
                // `start` is a cmd builtin; the empty "" is the window title arg
                // so a URL with spaces isn't mistaken for the title.
                (
                    "cmd".to_string(),
                    vec![
                        "/C".to_string(),
                        "start".to_string(),
                        String::new(),
                        url.to_string(),
                    ],
                )
            } else {
                ("xdg-open".to_string(), vec![url.to_string()])
            }
        }
    }
}

/// Fire-and-forget browser launch (async, for the daemon). Spawns and **drops**
/// the child without awaiting it — the opener (xdg-open/open/start) returns
/// promptly. Best-effort: returns the spawn error for the caller to log at
/// debug; never blocks the async runtime and never panics.
pub async fn open_url(browser: Option<&str>, url: &str) -> std::io::Result<()> {
    let (program, args) = browser_command(browser, url);
    let child = tokio::process::Command::new(&program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    drop(child); // detach; tokio reaps the orphan via its process driver
    debug!(program = %program, url = %url, "launched browser (best-effort)");
    Ok(())
}

/// Synchronous sibling of [`open_url`] for non-async call sites (the static
/// `--ports-events` path). Same fire-and-forget, best-effort contract.
pub fn open_url_blocking(browser: Option<&str>, url: &str) -> std::io::Result<()> {
    let (program, args) = browser_command(browser, url);
    let child = std::process::Command::new(&program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    drop(child); // detach; the short-lived `up` process exits soon after
    debug!(program = %program, url = %url, "launched browser (best-effort)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(browser: Option<&str>) -> Settings {
        Settings {
            host_ca: None,
            browser: browser.map(String::from),
        }
    }

    #[test]
    fn resolve_prefers_env_over_settings() {
        let s = settings_with(Some("from-settings"));
        assert_eq!(
            resolve_browser(Some("from-env"), &s).as_deref(),
            Some("from-env")
        );
    }

    #[test]
    fn resolve_uses_settings_when_env_unset_or_empty() {
        let s = settings_with(Some("firefox"));
        assert_eq!(resolve_browser(None, &s).as_deref(), Some("firefox"));
        assert_eq!(resolve_browser(Some("   "), &s).as_deref(), Some("firefox"));
    }

    #[test]
    fn resolve_none_when_both_unset() {
        assert_eq!(resolve_browser(None, &settings_with(None)), None);
        assert_eq!(resolve_browser(Some(""), &settings_with(Some("  "))), None);
    }

    #[test]
    fn command_uses_configured_program_verbatim() {
        let (prog, args) = browser_command(Some("firefox"), "http://127.0.0.1:3000");
        assert_eq!(prog, "firefox");
        assert_eq!(args, vec!["http://127.0.0.1:3000".to_string()]);
    }

    #[test]
    fn command_default_opener_for_this_platform() {
        let (prog, args) = browser_command(None, "http://127.0.0.1:8080");
        if cfg!(target_os = "macos") {
            assert_eq!(prog, "open");
            assert_eq!(args, vec!["http://127.0.0.1:8080".to_string()]);
        } else if cfg!(target_os = "windows") {
            assert_eq!(prog, "cmd");
            assert_eq!(args.first().map(String::as_str), Some("/C"));
            assert_eq!(
                args.last().map(String::as_str),
                Some("http://127.0.0.1:8080")
            );
        } else {
            assert_eq!(prog, "xdg-open");
            assert_eq!(args, vec!["http://127.0.0.1:8080".to_string()]);
        }
    }
}
