//! Real-world corpus manifest (`cor-`) — `conformance/discovery/corpus.json`
//! (025-exploratory-parity-discovery, data-model.md § 8, US7).
//!
//! The manifest is Rust-owned strict JSON rather than a Python tuple so the
//! immutable-reference check (**D4**) runs **hermetically**, on every pull request,
//! without network access: a validation that only runs when the network is up is a
//! validation that does not run (research D8). Corpus *content* is never vendored — this
//! file records provenance, not bytes (FR-053).
//!
//! ## What **D4** actually asserts
//!
//! Two clauses, checked in two different places because they are answerable in two
//! different places:
//!
//! 1. **Non-immutable reference** (FR-050) — a branch, a moving tag, `HEAD`, `latest`, an
//!    abbreviated SHA, or anything else that is not a 40-hex object name. This is a
//!    property of the manifest *alone*, so [`check`] answers it from a single load,
//!    hermetically, with no network and no history. That is the whole reason the manifest
//!    moved into Rust.
//! 2. **A digest recorded and then removed** — a `contentDigest` that was `sha256:…` and
//!    is now `null`. This is a property of a *change*, not of a file, so no single-load
//!    check can see it. [`check_drift`] answers it against an explicit baseline, and the
//!    fetch path (`parity_harness::discovery::corpus_fetch`) is the caller that has one:
//!    it holds the committed manifest while it materializes the new digests.
//!
//! Splitting them is deliberate rather than an omission. Folding clause 2 into [`check`]
//! would require the checker to reconstruct history — from git, or from a second committed
//! copy — and both make a hermetic validator depend on something outside the file it
//! validates.
//!
//! ## Why the id is derived
//!
//! `cor-<hash8(repository ‖ commit ‖ path)>` (data-model.md § 0). Derivation makes two
//! entries naming the same upstream workspace *unrepresentable*: they collide on id, and
//! the check rejects the duplicate. A hand-chosen id would let one snapshot be fetched,
//! digested, and compared twice under two names, and a divergence found through both would
//! look like two.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::DiscoveryError;
use crate::discovery::hash8;

/// Length of a git object name in hex characters.
const OBJECT_NAME_LEN: usize = 40;

/// The `sha256:` prefix every recorded content digest carries.
pub const DIGEST_PREFIX: &str = "sha256:";

/// Length of the hex body of a SHA-256 digest.
const DIGEST_BODY_LEN: usize = 64;

/// One pinned third-party workspace (data-model.md § 8).
///
/// Field order here is the emitted JSON field order, so recording a digest produces a
/// reviewable one-line diff rather than a reformat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorpusEntry {
    /// Derived, never authored: `cor-<hash8(repository ‖ commit ‖ path)>`.
    pub id: String,
    /// The short human name the entry is referred to by (`try-node`, `oss-ruff`, …), and
    /// the name its frozen `realworld::<name>` baseline unit carries.
    pub name: String,
    /// `owner/repo` on GitHub.
    pub repository: String,
    /// The **immutable** commit: a 40-hex object name. A branch, a tag, `HEAD`, or
    /// `latest` is **D4** (FR-050).
    pub commit: String,
    /// The workspace root within the repository. Empty means the repository root.
    pub path: String,
    /// `sha256:<64-hex>` over the materialized workspace, or `null` until the entry has
    /// been materialized once. Verified on every later fetch (FR-051).
    pub content_digest: Option<String>,
    /// Why this workspace was selected, and anything about its shape a reader needs.
    pub notes: String,
}

impl CorpusEntry {
    /// The derived id: `cor-<hash8(repository ‖ commit ‖ path)>`.
    ///
    /// Those three parts are exactly what identifies an upstream workspace snapshot.
    /// `name` is deliberately **not** a part: renaming an entry for clarity must not
    /// re-key it, or the rename would read as one snapshot removed and another added.
    pub fn derive_id(repository: &str, commit: &str, path: &str) -> String {
        format!("cor-{}", hash8(&[repository, commit, path]))
    }

    /// This entry's id as derived from its own substance.
    pub fn derived_id(&self) -> String {
        CorpusEntry::derive_id(&self.repository, &self.commit, &self.path)
    }

    /// The directory this entry materializes into, under `root`.
    ///
    /// Keyed on the **id** rather than the name so two entries can never share a
    /// directory: the id is a function of the upstream identity, the name is a label.
    pub fn workspace_dir(&self, root: &Path) -> std::path::PathBuf {
        root.join(&self.id)
    }
}

/// Whether `reference` is an immutable git object name (FR-050).
///
/// Exactly 40 **lowercase** hex characters. Uppercase is rejected rather than folded:
/// git renders object names in lowercase, so an uppercase spelling means the value was
/// retyped or transformed by hand, and a manifest whose pins have been retyped is exactly
/// the one worth looking at twice. Everything else — `main`, `v1.2.3`, `HEAD`, `latest`,
/// an abbreviated SHA — is mutable or ambiguous and therefore refused.
pub fn is_immutable_reference(reference: &str) -> bool {
    reference.len() == OBJECT_NAME_LEN
        && reference
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Whether `digest` is a well-formed `sha256:<64 lowercase hex>` value.
pub fn is_well_formed_digest(digest: &str) -> bool {
    let Some(body) = digest.strip_prefix(DIGEST_PREFIX) else {
        return false;
    };
    body.len() == DIGEST_BODY_LEN
        && body
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// `sha256:<64-hex>` over a materialized workspace tree, keyed by workspace-relative
/// POSIX path.
///
/// Lives here rather than beside the fetch so the digest **format** and the digest
/// **computation** are one definition: [`is_well_formed_digest`] and this function must
/// never be able to disagree about what a recorded digest looks like.
///
/// Every part is **length-prefixed** before hashing, for the same reason
/// [`hash8`] is: a path and a payload concatenated without a length are not injective, so
/// two different trees could digest identically — and a verification that cannot
/// distinguish them verifies nothing. A separator byte is not enough here either, because
/// file *contents* are arbitrary bytes and can contain any separator.
pub fn digest_of(content: &std::collections::BTreeMap<String, Vec<u8>>) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update((content.len() as u64).to_le_bytes());
    for (path, bytes) in content {
        hasher.update((path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    format!("{DIGEST_PREFIX}{:x}", hasher.finalize())
}

/// The `corpus.json` envelope. `records` is mandatory for the same reason as the findings
/// queue's: a truncated file must not load as "this repository pins no corpus".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorpusFile {
    /// Schema version of the file format — rejected at load unless it is the current
    /// [`SCHEMA_VERSION`](crate::discovery::queue::SCHEMA_VERSION).
    #[serde(deserialize_with = "supported_schema_version")]
    pub schema_version: u32,
    /// The pinned entries, in file order.
    pub records: Vec<CorpusEntry>,
}

impl Default for CorpusFile {
    fn default() -> Self {
        CorpusFile {
            schema_version: crate::discovery::queue::SCHEMA_VERSION,
            records: Vec::new(),
        }
    }
}

fn supported_schema_version<'de, D>(de: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u32::deserialize(de)?;
    let current = crate::discovery::queue::SCHEMA_VERSION;
    if value != current {
        return Err(serde::de::Error::custom(format!(
            "unsupported schemaVersion {value}: this build reads and writes version \
             {current}, and writing would stamp {current} over it"
        )));
    }
    Ok(value)
}

/// Render a corpus file in its canonical, byte-stable form (2-space pretty JSON,
/// trailing newline) — the same rendering every machine-touched artifact in this crate
/// uses, so recording a digest produces a reviewable one-line diff.
pub fn render(file: &CorpusFile) -> String {
    let mut out = serde_json::to_string_pretty(file)
        .unwrap_or_else(|e| unreachable!("corpus record serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Atomically write the corpus manifest to `dir/corpus.json`.
///
/// Delegates to the single [`crate::atomic_write`] primitive (unique temp file +
/// `fs::rename`): a shorter payload written over a longer one must never leave trailing
/// bytes.
pub fn write(dir: &Path, entries: &[CorpusEntry]) -> std::io::Result<()> {
    let file = CorpusFile {
        schema_version: crate::discovery::queue::SCHEMA_VERSION,
        records: entries.to_vec(),
    };
    crate::atomic_write(&crate::discovery::queue::corpus_path(dir), &render(&file))
}

/// **D4** over a loaded manifest — everything answerable from the file alone.
///
/// Four shapes, one class, because all four are the same defect: an entry that does not
/// name a retrievable, verifiable snapshot.
///
/// - a `commit` that is not a 40-hex object name (FR-050);
/// - a `contentDigest` that is present but not `sha256:<64-hex>` — a malformed digest is
///   not a weaker check, it is one that can never disagree;
/// - an `id` that does not derive from `repository ‖ commit ‖ path`, which detaches the
///   record from the snapshot it claims to identify;
/// - a duplicate id or name, which makes one snapshot two entries.
pub fn check(entries: &[CorpusEntry]) -> Vec<DiscoveryError> {
    let mut out = Vec::new();
    let mut seen_ids: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut seen_names: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();

    for entry in entries {
        if !is_immutable_reference(&entry.commit) {
            out.push(DiscoveryError::CorpusIntegrity {
                record: entry.id.clone(),
                cause: format!(
                    "`commit` is `{}`, which is not a 40-character lowercase-hex object \
                     name. A branch, a tag, `HEAD`, `latest`, or an abbreviated SHA names \
                     different content tomorrow, so a finding recorded against it is a \
                     claim about content nobody can retrieve",
                    entry.commit
                ),
            });
        }
        if let Some(digest) = &entry.content_digest
            && !is_well_formed_digest(digest)
        {
            out.push(DiscoveryError::CorpusIntegrity {
                record: entry.id.clone(),
                cause: format!(
                    "`contentDigest` is `{digest}`, which is not `{DIGEST_PREFIX}<64 \
                     lowercase hex>`. A malformed digest is not a weaker verification, it \
                     is one that can never disagree"
                ),
            });
        }
        let derived = entry.derived_id();
        if entry.id != derived {
            out.push(DiscoveryError::CorpusIntegrity {
                record: entry.id.clone(),
                cause: format!(
                    "id does not match its substance (expected `{derived}` from `{}` ‖ \
                     `{}` ‖ `{}`)",
                    entry.repository, entry.commit, entry.path
                ),
            });
        }
        if !seen_ids.insert(entry.id.as_str()) {
            out.push(DiscoveryError::CorpusIntegrity {
                record: entry.id.clone(),
                cause: "duplicate corpus entry id — every by-id lookup takes the first \
                        match, so the second record would be fetched, digested, and \
                        compared under an identity nothing resolves to"
                    .to_string(),
            });
        }
        if !seen_names.insert(entry.name.as_str()) {
            out.push(DiscoveryError::CorpusIntegrity {
                record: entry.id.clone(),
                cause: format!(
                    "duplicate corpus entry name `{}` — the name is what a reviewer and \
                     the frozen `realworld::<name>` baseline units refer to, and two \
                     entries under one name make that reference ambiguous",
                    entry.name
                ),
            });
        }
    }
    out
}

/// **D4**'s second clause: a digest that was recorded and then removed.
///
/// Answerable only against a baseline, so it is a separate entry point that takes one
/// explicitly (see this module's header). `previous` is the manifest as committed;
/// `current` is the manifest about to be written.
///
/// Re-recording a *different* digest is deliberately **not** flagged here — that is the
/// FR-051 mismatch, which the fetch reports for the entry against real content. Silently
/// dropping the digest is different in kind: it does not disagree with anything, it
/// deletes the thing a later fetch would have disagreed with.
pub fn check_drift(previous: &[CorpusEntry], current: &[CorpusEntry]) -> Vec<DiscoveryError> {
    let mut out = Vec::new();
    for before in previous {
        let Some(digest) = &before.content_digest else {
            continue;
        };
        let Some(after) = current.iter().find(|e| e.id == before.id) else {
            // A removed entry is a deliberate re-pin, not a lost digest: the id is a
            // function of the commit, so re-pinning necessarily removes the old record,
            // and the digest goes with the snapshot it described.
            continue;
        };
        if after.content_digest.is_none() {
            out.push(DiscoveryError::CorpusIntegrity {
                record: before.id.clone(),
                cause: format!(
                    "`contentDigest` was recorded as `{digest}` and is now null. A digest \
                     is recorded once, at first materialization, and verified on every \
                     later fetch; removing it does not weaken the check, it deletes the \
                     only thing a later fetch could have disagreed with"
                ),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(repository: &str, commit: &str, path: &str) -> CorpusEntry {
        CorpusEntry {
            id: CorpusEntry::derive_id(repository, commit, path),
            name: format!("{repository}@{path}"),
            repository: repository.to_string(),
            commit: commit.to_string(),
            path: path.to_string(),
            content_digest: None,
            notes: String::new(),
        }
    }

    const SHA: &str = "31b61b521d55926d62c748b659f24ae71774c0e3";

    #[test]
    fn only_a_40_hex_lowercase_object_name_is_immutable() {
        assert!(is_immutable_reference(SHA));
        // Every floating reference FR-050 names, plus the near-misses.
        for mutable in [
            "main",
            "master",
            "HEAD",
            "latest",
            "v1.2.3",
            "refs/heads/main",
            "31b61b5",                                   // abbreviated
            "31B61B521D55926D62C748B659F24AE71774C0E3",  // uppercase
            "31b61b521d55926d62c748b659f24ae71774c0e",   // 39
            "31b61b521d55926d62c748b659f24ae71774c0e33", // 41
            "31b61b521d55926d62c748b659f24ae71774c0g3",  // non-hex
            "",
        ] {
            assert!(
                !is_immutable_reference(mutable),
                "`{mutable}` must not pass as an immutable reference"
            );
        }
    }

    #[test]
    fn a_mutable_reference_is_d4() {
        let e = entry("microsoft/vscode-remote-try-node", "main", "");
        let violations = check(std::slice::from_ref(&e));
        assert!(
            violations
                .iter()
                .any(|v| v.class() == "D4"
                    && v.to_string().contains("not a 40-character lowercase-hex")),
            "a branch reference must be D4: {violations:?}"
        );
    }

    #[test]
    fn a_pinned_entry_with_no_digest_is_clean() {
        // `contentDigest: null` is the state of every entry before its first
        // materialization. It must not be a violation, or the manifest could never be
        // authored in the first place.
        assert!(check(&[entry("devcontainers/images", SHA, "src/go")]).is_empty());
    }

    #[test]
    fn a_malformed_digest_is_d4() {
        let mut e = entry("devcontainers/images", SHA, "src/go");
        e.content_digest = Some("deadbeef".to_string());
        assert!(
            check(std::slice::from_ref(&e))
                .iter()
                .any(|v| v.class() == "D4" && v.to_string().contains("contentDigest"))
        );
        e.content_digest = Some(format!("{DIGEST_PREFIX}{}", "a".repeat(64)));
        assert!(check(std::slice::from_ref(&e)).is_empty());
    }

    #[test]
    fn a_hand_chosen_id_is_d4() {
        let mut e = entry("devcontainers/images", SHA, "src/go");
        e.id = "cor-deadbeef".to_string();
        assert!(
            check(std::slice::from_ref(&e)).iter().any(
                |v| v.class() == "D4" && v.to_string().contains("does not match its substance")
            )
        );
    }

    #[test]
    fn two_entries_naming_one_snapshot_collide_on_id() {
        let a = entry("devcontainers/images", SHA, "src/go");
        let b = entry("devcontainers/images", SHA, "src/go");
        assert_eq!(a.id, b.id);
        assert!(
            check(&[a, b])
                .iter()
                .any(|v| v.to_string().contains("duplicate corpus entry id"))
        );
    }

    #[test]
    fn a_removed_digest_is_d4_against_the_baseline() {
        let mut before = entry("devcontainers/images", SHA, "src/go");
        before.content_digest = Some(format!("{DIGEST_PREFIX}{}", "b".repeat(64)));
        let after = entry("devcontainers/images", SHA, "src/go");

        let violations = check_drift(std::slice::from_ref(&before), std::slice::from_ref(&after));
        assert!(
            violations
                .iter()
                .any(|v| v.class() == "D4" && v.to_string().contains("is now null")),
            "removing a recorded digest must be D4: {violations:?}"
        );

        // Re-recording the SAME digest, or a different one, is not this clause: a
        // disagreement is the FR-051 mismatch the fetch reports against real content.
        let mut same = after.clone();
        same.content_digest = before.content_digest.clone();
        assert!(check_drift(std::slice::from_ref(&before), &[same]).is_empty());
        let mut different = after.clone();
        different.content_digest = Some(format!("{DIGEST_PREFIX}{}", "c".repeat(64)));
        assert!(check_drift(std::slice::from_ref(&before), &[different]).is_empty());

        // A re-pin REMOVES the entry (its id is a function of the commit), and the digest
        // goes with the snapshot it described.
        assert!(check_drift(std::slice::from_ref(&before), &[]).is_empty());
    }

    #[test]
    fn the_id_ignores_the_name_but_not_the_upstream_identity() {
        let a = entry("devcontainers/images", SHA, "src/go");
        let mut renamed = a.clone();
        renamed.name = "something-else".to_string();
        assert_eq!(a.id, renamed.derived_id(), "a rename must not re-key");

        assert_ne!(a.id, entry("devcontainers/images", SHA, "src/rust").id);
        assert_ne!(a.id, entry("devcontainers/templates", SHA, "src/go").id);
        assert_ne!(
            a.id,
            entry(
                "devcontainers/images",
                "0000000000000000000000000000000000000000",
                "src/go"
            )
            .id
        );
    }

    #[test]
    fn the_rendered_file_is_byte_stable_and_newline_terminated() {
        let file = CorpusFile {
            schema_version: crate::discovery::queue::SCHEMA_VERSION,
            records: vec![entry("devcontainers/images", SHA, "src/go")],
        };
        let once = render(&file);
        assert_eq!(once, render(&file));
        assert!(once.ends_with("}\n"));
        let round: CorpusFile = serde_json::from_str(&once).expect("round-trips");
        assert_eq!(round, file);
    }

    #[test]
    fn an_unknown_field_and_a_future_schema_version_are_both_refused() {
        let unknown = serde_json::json!({
            "schemaVersion": crate::discovery::queue::SCHEMA_VERSION,
            "records": [],
            "extra": true
        });
        assert!(serde_json::from_value::<CorpusFile>(unknown).is_err());

        let future = serde_json::json!({
            "schemaVersion": crate::discovery::queue::SCHEMA_VERSION + 1,
            "records": []
        });
        assert!(serde_json::from_value::<CorpusFile>(future).is_err());
    }
}
