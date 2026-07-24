//! Filesystem observer (`chan-filesystem` / `chan-file-content`): presence, attributes,
//! and contents of the case's declared `fsAllowlist` paths — ALLOWLIST-SCOPED, never a
//! full-tree diff (clarify Q1, FR-010, T044).
//!
//! Capture is rooted at [`RunContext::workspace`] and covers ONLY the paths in
//! [`RunContext::fs_allowlist`]. `path_token` is applied by the shared normalizer
//! afterward. The raw value keys are workspace-RELATIVE paths (portable already); file
//! modes and contents are captured verbatim.

use std::path::Path;

use deacon_conformance::model::{CHAN_FILE_CONTENT, CHAN_FILESYSTEM, Operation};
use serde_json::{Map, Value};

use crate::HarnessError;
use crate::evidence::RawChannelEvidence;
use crate::observe::{ChannelObserver, RunContext};

/// Observer for the two filesystem channels. One instance per channel.
#[derive(Debug, Clone, Copy)]
pub struct FilesystemObserver {
    channel: &'static str,
}

impl FilesystemObserver {
    /// Construct the observer for a filesystem channel, or `None` otherwise.
    pub fn for_channel(channel: &str) -> Option<FilesystemObserver> {
        let channel = match channel {
            CHAN_FILESYSTEM => CHAN_FILESYSTEM,
            CHAN_FILE_CONTENT => CHAN_FILE_CONTENT,
            _ => return None,
        };
        Some(FilesystemObserver { channel })
    }

    /// Capture the allowlisted paths under `workspace` for this channel. Kept separate
    /// from the trait so unit tests can drive it without a full [`RunContext`].
    pub fn capture_scoped(
        &self,
        op_id: &str,
        workspace: &Path,
        allowlist: &[String],
    ) -> RawChannelEvidence {
        let mut map = Map::new();
        for rel in allowlist {
            let abs = workspace.join(rel);
            let entry = match self.channel {
                CHAN_FILESYSTEM => filesystem_entry(&abs),
                CHAN_FILE_CONTENT => file_content_entry(&abs),
                _ => Value::Null,
            };
            map.insert(rel.clone(), entry);
        }
        RawChannelEvidence {
            channel: self.channel.to_string(),
            operation: op_id.to_string(),
            // `present:false` only when the case declared NO allowlist for this channel;
            // an allowlisted-but-absent path is captured as `{exists:false}` (distinct).
            present: !allowlist.is_empty(),
            value: Value::Object(map),
        }
    }
}

impl ChannelObserver for FilesystemObserver {
    fn channel(&self) -> &'static str {
        self.channel
    }

    fn capture(
        &self,
        ctx: &RunContext,
        op: &Operation,
    ) -> Result<RawChannelEvidence, HarnessError> {
        Ok(self.capture_scoped(&op.id, &ctx.workspace, &ctx.fs_allowlist))
    }
}

/// `chan-filesystem` entry: `{ "exists": bool, "mode": "0644"|null }`. An absent path is
/// captured (`exists:false`), never dropped (FR-018/FR-029).
fn filesystem_entry(abs: &Path) -> Value {
    match std::fs::metadata(abs) {
        Ok(meta) => serde_json::json!({ "exists": true, "mode": mode_string(&meta) }),
        Err(_) => serde_json::json!({ "exists": false, "mode": Value::Null }),
    }
}

/// `chan-file-content` entry: the file's UTF-8 content, or `null` when absent/unreadable
/// (captured-but-empty stays distinct from a captured empty string).
fn file_content_entry(abs: &Path) -> Value {
    match std::fs::read_to_string(abs) {
        Ok(content) => Value::String(content),
        Err(_) => Value::Null,
    }
}

/// The file mode as a 4-digit octal string on Unix; `null` elsewhere (mode is a
/// Unix-only attribute — matching the repo's cross-platform convention).
#[cfg(unix)]
fn mode_string(meta: &std::fs::Metadata) -> Value {
    use std::os::unix::fs::MetadataExt;
    Value::String(format!("{:04o}", meta.mode() & 0o7777))
}

#[cfg(not(unix))]
fn mode_string(_meta: &std::fs::Metadata) -> Value {
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_present_and_absent_allowlisted_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".devcontainer")).unwrap();
        std::fs::write(dir.path().join(".devcontainer/devcontainer.json"), "{}").unwrap();

        let obs = FilesystemObserver::for_channel(CHAN_FILESYSTEM).unwrap();
        let ev = obs.capture_scoped(
            "op",
            dir.path(),
            &[
                ".devcontainer/devcontainer.json".to_string(),
                "does-not-exist".to_string(),
            ],
        );
        assert!(ev.present, "a declared allowlist → present");
        assert_eq!(ev.value[".devcontainer/devcontainer.json"]["exists"], true);
        assert_eq!(
            ev.value["does-not-exist"]["exists"], false,
            "an absent allowlisted path is captured, not dropped (FR-018)"
        );
    }

    #[test]
    fn empty_allowlist_is_present_false() {
        let dir = tempfile::tempdir().expect("tempdir");
        let obs = FilesystemObserver::for_channel(CHAN_FILESYSTEM).unwrap();
        let ev = obs.capture_scoped("op", dir.path(), &[]);
        assert!(!ev.present, "no allowlist declared → channel not captured");
    }

    #[test]
    fn file_content_captured_and_absent_is_null() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("marker.txt"), "hello").unwrap();
        let obs = FilesystemObserver::for_channel(CHAN_FILE_CONTENT).unwrap();
        let ev = obs.capture_scoped(
            "op",
            dir.path(),
            &["marker.txt".to_string(), "gone.txt".to_string()],
        );
        assert_eq!(ev.value["marker.txt"], Value::String("hello".to_string()));
        assert_eq!(
            ev.value["gone.txt"],
            Value::Null,
            "absent file → null content"
        );
    }

    #[test]
    fn for_channel_rejects_non_fs_channels() {
        assert!(FilesystemObserver::for_channel("chan-exit-code").is_none());
        assert!(FilesystemObserver::for_channel(CHAN_FILE_CONTENT).is_some());
    }
}
