//! Network-lane corpus fetch with content-digest verification
//! (025-exploratory-parity-discovery, US7, T107).
//!
//! The only network-touching code in the feature. A digest is recorded on **first**
//! materialization and verified on every later fetch (FR-051); a mismatch fails that
//! entry loudly rather than comparing against unexpected content, and an unreachable
//! entry is reported as unreachable rather than as "ran and found nothing" (FR-052).
//!
//! ## Why `git`, and why a partial clone
//!
//! The retired Python fetcher walked GitHub's contents API one file at a time through
//! `gh`, which needs an authenticated CLI and burns a rate-limit budget that 33 entries
//! exhaust immediately. This uses `git` directly with a **blob-filtered partial clone plus
//! a sparse checkout**, so one network round trip per entry materializes only the
//! devcontainer subtree — `microsoft/vscode` costs the same as `vscode-remote-try-node`.
//! No API token, no rate limit, and `git` is already a prerequisite of working in this
//! repository at all.
//!
//! Only `<path>/.devcontainer/**` and `<path>/.devcontainer.json` are materialized. That
//! is not a shortcut: the corpus tier is a **configuration-resolution** differential
//! (research D10), so an entry's application sources take part in nothing being compared,
//! and fetching them would trade the tier's budget for bytes neither implementation reads.
//!
//! ## Three outcomes, never two
//!
//! [`EntryStatus`] deliberately has no "failed" catch-all. `Unreachable` and
//! `DigestMismatch` are different facts — the first says the snapshot could not be
//! retrieved, the second says it was retrieved and is not what was recorded — and
//! collapsing them would let content drift at a pinned commit read as a flaky network.
//!
//! A genuine *machinery* failure (an unwritable destination) is an `Err`, because it is a
//! statement about the run rather than about an entry.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use deacon_conformance::discovery::corpus::{self, CorpusEntry, digest_of};

use crate::HarnessError;

/// Path override for the `git` binary — the fault-injection seam.
///
/// Public and explicit for the same reason [`crate::prereq::probe_docker`] is: the
/// alternative is a test calling `std::env::set_var`, which is `unsafe` under this
/// workspace's edition (and `unsafe_code = "deny"` forbids), besides being process-global
/// and hostile to a parallel runner.
pub const GIT_OVERRIDE_ENV: &str = "DEACON_DISCOVERY_GIT";

/// Bound on the `git version` prerequisite probe.
pub const GIT_PROBE_BOUND: Duration = Duration::from_secs(60);

/// Bound on one entry's network fetch. Generous — a cold partial clone of a large
/// repository is still a single round trip, and a bound tight enough to trip on a slow
/// runner would report a healthy entry as unreachable.
pub const DEFAULT_ENTRY_BOUND: Duration = Duration::from_secs(300);

/// The `git` binary this run uses.
pub fn git_binary() -> PathBuf {
    std::env::var_os(GIT_OVERRIDE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("git"))
}

/// Require a working `git`. Any failure is [`HarnessError::NetworkUnavailable`].
pub async fn require_git() -> Result<(), HarnessError> {
    probe_git(&git_binary(), GIT_PROBE_BOUND).await
}

/// Probe a specific `git` binary. Pure over its inputs so fault injection can point it at
/// a failing stub without touching process environment.
pub async fn probe_git(bin: &Path, bound: Duration) -> Result<(), HarnessError> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);

    match tokio::time::timeout(bound, cmd.status()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(HarnessError::NetworkUnavailable {
            cause: format!("`{} version` exited with {status}", bin.display()),
        }),
        Ok(Err(e)) => Err(HarnessError::NetworkUnavailable {
            cause: format!("could not run `{} version`: {e}", bin.display()),
        }),
        Err(_elapsed) => Err(HarnessError::NetworkUnavailable {
            cause: format!(
                "`{} version` did not answer within {bound:?}",
                bin.display()
            ),
        }),
    }
}

/// One entry that was retrieved and whose digest is settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Materialized {
    /// The entry's `cor-` id.
    pub entry_id: String,
    /// The entry's human name.
    pub name: String,
    /// The materialized workspace root — what both implementations are pointed at.
    pub workspace: PathBuf,
    /// The digest computed over the materialized content, `sha256:<64-hex>`.
    pub digest: String,
    /// **True only on first materialization**, when the manifest carried `null` and this
    /// run recorded the digest. Every later fetch verifies instead, so a `true` here on a
    /// second run means the digest was removed — the second clause of **D4**.
    pub recorded: bool,
}

/// What happened to one corpus entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryStatus {
    /// Retrieved, and its digest either verified or recorded for the first time.
    Materialized(Materialized),
    /// **FR-051** — retrieved, but the content digest disagrees with the recorded one.
    /// The entry is not compared: comparing against unexpected content would attribute a
    /// difference in the *upstream workspace* to a difference between the implementations.
    DigestMismatch {
        entry_id: String,
        name: String,
        expected: String,
        actual: String,
    },
    /// **FR-052** — the snapshot could not be retrieved. Distinguished from an entry that
    /// ran and produced no finding, because the two are opposites: one says nothing was
    /// compared, the other says everything compared agreed.
    Unreachable {
        entry_id: String,
        name: String,
        cause: String,
    },
}

impl EntryStatus {
    /// The entry this status is about.
    pub fn entry_id(&self) -> &str {
        match self {
            EntryStatus::Materialized(m) => &m.entry_id,
            EntryStatus::DigestMismatch { entry_id, .. }
            | EntryStatus::Unreachable { entry_id, .. } => entry_id,
        }
    }

    /// A one-line rendering for a campaign log or report.
    pub fn summary(&self) -> String {
        match self {
            EntryStatus::Materialized(m) => format!(
                "{} ({}): {} {}",
                m.name,
                m.entry_id,
                if m.recorded { "recorded" } else { "verified" },
                m.digest
            ),
            EntryStatus::DigestMismatch {
                entry_id,
                name,
                expected,
                actual,
            } => format!(
                "{name} ({entry_id}): DIGEST MISMATCH — recorded {expected}, materialized \
                 {actual}"
            ),
            EntryStatus::Unreachable {
                entry_id,
                name,
                cause,
            } => format!("{name} ({entry_id}): UNREACHABLE — {cause}"),
        }
    }
}

/// Materialize one corpus entry under `root` and settle its digest.
///
/// `Err` is reserved for machinery failures — a destination that cannot be created is a
/// statement about the run, not about the entry. Everything an entry can do wrong comes
/// back as an [`EntryStatus`].
pub async fn materialize(
    entry: &CorpusEntry,
    root: &Path,
    bound: Duration,
) -> Result<EntryStatus, HarnessError> {
    // A mutable reference never reaches the network. **D4** already rejects it
    // hermetically, but the fetch refuses it too: this function is reachable from a caller
    // holding a hand-built entry, and fetching a branch would record a digest over content
    // that is different tomorrow — a verification that quietly means nothing.
    if !corpus::is_immutable_reference(&entry.commit) {
        return Ok(EntryStatus::Unreachable {
            entry_id: entry.id.clone(),
            name: entry.name.clone(),
            cause: format!(
                "`{}` is not a 40-character lowercase-hex object name, so it names \
                 different content over time and is never fetched",
                entry.commit
            ),
        });
    }

    let workspace = entry.workspace_dir(root);
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace).map_err(|e| HarnessError::Report {
            cause: format!("could not clear {}: {e}", workspace.display()),
        })?;
    }
    std::fs::create_dir_all(&workspace).map_err(|e| HarnessError::Report {
        cause: format!("could not create {}: {e}", workspace.display()),
    })?;

    if let Err(cause) = clone_sparse(entry, &workspace, bound).await {
        return Ok(EntryStatus::Unreachable {
            entry_id: entry.id.clone(),
            name: entry.name.clone(),
            cause,
        });
    }

    let files = collect_content(&workspace).map_err(|e| HarnessError::Report {
        cause: format!(
            "could not read the materialized workspace {}: {e}",
            workspace.display()
        ),
    })?;
    if files.is_empty() {
        // The commit resolved and the checkout succeeded, but the pinned path holds no
        // devcontainer configuration. That is "the workspace this entry names is not
        // there", which is unreachability — NOT an agreement between two implementations
        // that were never invoked.
        return Ok(EntryStatus::Unreachable {
            entry_id: entry.id.clone(),
            name: entry.name.clone(),
            cause: format!(
                "the pinned path `{}` carries no devcontainer configuration at commit {}",
                if entry.path.is_empty() {
                    "<repository root>"
                } else {
                    &entry.path
                },
                entry.commit
            ),
        });
    }

    let digest = digest_of(&files);
    match &entry.content_digest {
        None => Ok(EntryStatus::Materialized(Materialized {
            entry_id: entry.id.clone(),
            name: entry.name.clone(),
            workspace,
            digest,
            recorded: true,
        })),
        Some(expected) if *expected == digest => Ok(EntryStatus::Materialized(Materialized {
            entry_id: entry.id.clone(),
            name: entry.name.clone(),
            workspace,
            digest,
            recorded: false,
        })),
        Some(expected) => Ok(EntryStatus::DigestMismatch {
            entry_id: entry.id.clone(),
            name: entry.name.clone(),
            expected: expected.clone(),
            actual: digest,
        }),
    }
}

/// Fold the digests a run settled back into the manifest, and write it.
///
/// Only **first** materializations change anything: a verified digest is already what the
/// file says, and a mismatched one is deliberately not written (FR-051 — the recorded
/// digest is the claim under test, so overwriting it with whatever was fetched would turn
/// every mismatch into a silent re-baseline).
///
/// The write is refused outright if it would drop a digest the committed manifest already
/// carries — **D4**'s second clause, checked here because this is the one caller that
/// holds both the baseline and the successor (see [`corpus::check_drift`]).
pub fn record_digests(
    discovery_dir: &Path,
    committed: &[CorpusEntry],
    statuses: &[EntryStatus],
) -> Result<Vec<String>, HarnessError> {
    let recorded: BTreeMap<&str, &str> = statuses
        .iter()
        .filter_map(|s| match s {
            EntryStatus::Materialized(m) if m.recorded => {
                Some((m.entry_id.as_str(), m.digest.as_str()))
            }
            _ => None,
        })
        .collect();
    if recorded.is_empty() {
        return Ok(Vec::new());
    }

    let mut next = committed.to_vec();
    let mut written = Vec::new();
    for entry in &mut next {
        if let Some(digest) = recorded.get(entry.id.as_str()) {
            entry.content_digest = Some((*digest).to_string());
            written.push(entry.id.clone());
        }
    }

    let drift = corpus::check_drift(committed, &next);
    if !drift.is_empty() {
        return Err(HarnessError::Report {
            cause: format!(
                "refusing to write the corpus manifest: {}",
                drift
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        });
    }

    corpus::write(discovery_dir, &next).map_err(|e| HarnessError::Report {
        cause: format!("could not write the corpus manifest: {e}"),
    })?;
    Ok(written)
}

// ---------------------------------------------------------------------------
// git
// ---------------------------------------------------------------------------

/// The sparse-checkout patterns for one entry: the devcontainer subtree, and nothing else.
///
/// `/` anchors each pattern at the repository root, so a `vendor/src/go/.devcontainer/`
/// buried in a monorepo cannot be matched by an entry pinned at `src/go`.
fn sparse_patterns(path: &str) -> Vec<String> {
    let prefix = path.trim_matches('/');
    if prefix.is_empty() {
        vec![
            "/.devcontainer/".to_string(),
            "/.devcontainer.json".to_string(),
        ]
    } else {
        vec![
            format!("/{prefix}/.devcontainer/"),
            format!("/{prefix}/.devcontainer.json"),
        ]
    }
}

/// Materialize the entry's devcontainer subtree into `dest`.
///
/// `Err(String)` is the *cause of unreachability*, never a run failure: every step depends
/// on a third party being up and a pin still resolving, and neither is something this
/// repository controls.
async fn clone_sparse(entry: &CorpusEntry, dest: &Path, bound: Duration) -> Result<(), String> {
    let git = git_binary();
    let url = format!("https://github.com/{}.git", entry.repository);

    run_git(&git, dest, &["init", "--quiet"], bound).await?;
    run_git(&git, dest, &["remote", "add", "origin", &url], bound).await?;
    run_git(
        &git,
        dest,
        &["config", "core.sparseCheckout", "true"],
        bound,
    )
    .await?;

    let mut patterns = sparse_patterns(&entry.path).join("\n");
    patterns.push('\n');
    let sparse_file = dest.join(".git").join("info").join("sparse-checkout");
    if let Some(parent) = sparse_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    std::fs::write(&sparse_file, patterns)
        .map_err(|e| format!("could not write {}: {e}", sparse_file.display()))?;

    // `--filter=blob:none` keeps the fetch to commits and trees; the checkout below then
    // pulls only the blobs the sparse patterns select. `--depth 1` on an explicit object
    // name is what makes a monorepo cost the same as a sample repository.
    run_git(
        &git,
        dest,
        &[
            "fetch",
            "--quiet",
            "--depth",
            "1",
            "--filter=blob:none",
            "origin",
            &entry.commit,
        ],
        bound,
    )
    .await?;
    run_git(&git, dest, &["checkout", "--quiet", "FETCH_HEAD"], bound).await?;
    Ok(())
}

async fn run_git(git: &Path, cwd: &Path, args: &[&str], bound: Duration) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(git);
    cmd.args(args)
        .current_dir(cwd)
        // A credential prompt would hang the campaign on a private or renamed repository
        // until the bound expired, and report an auth problem as a timeout.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let rendered = args.join(" ");
    match tokio::time::timeout(bound, cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => Ok(()),
        Ok(Ok(output)) => Err(format!(
            "`git {rendered}` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Ok(Err(e)) => Err(format!("could not run `git {rendered}`: {e}")),
        Err(_elapsed) => Err(format!("`git {rendered}` exceeded {bound:?}")),
    }
}

// ---------------------------------------------------------------------------
// digest
// ---------------------------------------------------------------------------

/// One materialized tree, keyed by workspace-relative POSIX path.
type Content = BTreeMap<String, Vec<u8>>;

/// Collect the materialized content, excluding `.git/`.
///
/// A `BTreeMap` rather than the walk order: the digest must not depend on the order a
/// filesystem happens to enumerate directories in, or the same snapshot would digest
/// differently on two machines and every verification would fail as a mismatch.
fn collect_content(root: &Path) -> std::io::Result<Content> {
    let mut out = Content::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            // `symlink_metadata`, not `metadata`: a symlink is digested as the link it is.
            // Following it would either digest the same bytes twice or escape the
            // materialized tree entirely.
            let meta = std::fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let key = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let bytes = if meta.is_symlink() {
                let target = std::fs::read_link(&path)?;
                let mut marked = b"symlink:".to_vec();
                marked.extend_from_slice(target.to_string_lossy().as_bytes());
                marked
            } else {
                std::fs::read(&path)?
            };
            out.insert(key, bytes);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_conformance::discovery::corpus::DIGEST_PREFIX;

    fn entry(path: &str, digest: Option<&str>) -> CorpusEntry {
        CorpusEntry {
            id: CorpusEntry::derive_id(
                "devcontainers/images",
                "31b61b521d55926d62c748b659f24ae71774c0e3",
                path,
            ),
            name: format!("images-{}", path.replace('/', "-")),
            repository: "devcontainers/images".to_string(),
            commit: "31b61b521d55926d62c748b659f24ae71774c0e3".to_string(),
            path: path.to_string(),
            content_digest: digest.map(str::to_string),
            notes: String::new(),
        }
    }

    #[test]
    fn sparse_patterns_are_anchored_at_the_repository_root() {
        assert_eq!(
            sparse_patterns(""),
            vec![
                "/.devcontainer/".to_string(),
                "/.devcontainer.json".to_string()
            ]
        );
        assert_eq!(
            sparse_patterns("src/go"),
            vec![
                "/src/go/.devcontainer/".to_string(),
                "/src/go/.devcontainer.json".to_string()
            ]
        );
        // A stray leading or trailing slash must not produce `//` or an unanchored
        // pattern: an unanchored `src/go/.devcontainer/` would also match a nested
        // `vendor/src/go/.devcontainer/` in a monorepo.
        assert_eq!(sparse_patterns("/src/go/"), sparse_patterns("src/go"));
    }

    #[test]
    fn the_digest_is_well_formed_and_order_independent() {
        let mut a = Content::new();
        a.insert(
            ".devcontainer/devcontainer.json".to_string(),
            b"{}".to_vec(),
        );
        a.insert(".devcontainer/Dockerfile".to_string(), b"FROM x".to_vec());

        let mut b = Content::new();
        b.insert(".devcontainer/Dockerfile".to_string(), b"FROM x".to_vec());
        b.insert(
            ".devcontainer/devcontainer.json".to_string(),
            b"{}".to_vec(),
        );

        let digest = digest_of(&a);
        assert_eq!(digest, digest_of(&b), "insertion order must not matter");
        assert!(
            corpus::is_well_formed_digest(&digest),
            "{digest} must satisfy the manifest's own digest format"
        );
    }

    #[test]
    fn the_digest_distinguishes_content_a_naive_concatenation_would_merge() {
        // The injectivity boundary case: the same bytes split differently across path and
        // payload. Without length prefixes these two trees hash identically, and a
        // verification that cannot tell them apart is one that cannot fail.
        let mut left = Content::new();
        left.insert("ab".to_string(), b"c".to_vec());
        let mut right = Content::new();
        right.insert("a".to_string(), b"bc".to_vec());
        assert_ne!(digest_of(&left), digest_of(&right));

        // And content changes must change the digest — the property FR-051 rests on.
        let mut before = Content::new();
        before.insert(
            ".devcontainer/devcontainer.json".to_string(),
            b"{}".to_vec(),
        );
        let mut after = Content::new();
        after.insert(
            ".devcontainer/devcontainer.json".to_string(),
            b"{\"image\":\"x\"}".to_vec(),
        );
        assert_ne!(digest_of(&before), digest_of(&after));

        // An empty tree has a digest too; it is never mistaken for a populated one.
        assert_ne!(digest_of(&Content::new()), digest_of(&before));
    }

    #[tokio::test]
    async fn a_mutable_reference_never_reaches_the_network() {
        let mut e = entry("src/go", None);
        e.commit = "main".to_string();
        let root = tempfile::tempdir().expect("tempdir");
        // No process is spawned at all: the refusal happens before `git` is invoked,
        // which is what makes this assertion meaningful with no network available.
        let status = materialize(&e, root.path(), Duration::from_secs(1))
            .await
            .expect("a refusal is a status, not a run failure");
        match status {
            EntryStatus::Unreachable { cause, .. } => {
                assert!(cause.contains("40-character lowercase-hex"), "{cause}");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[test]
    fn recording_only_writes_first_materializations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let committed = vec![entry("src/go", None), entry("src/rust", None)];
        corpus::write(dir.path(), &committed).expect("seed the manifest");

        let digest = format!("{DIGEST_PREFIX}{}", "a".repeat(64));
        let statuses = vec![
            EntryStatus::Materialized(Materialized {
                entry_id: committed[0].id.clone(),
                name: committed[0].name.clone(),
                workspace: dir.path().to_path_buf(),
                digest: digest.clone(),
                recorded: true,
            }),
            // A mismatch must never re-baseline: overwriting the recorded digest with
            // whatever was fetched would make every FR-051 failure self-healing, and
            // therefore invisible.
            EntryStatus::DigestMismatch {
                entry_id: committed[1].id.clone(),
                name: committed[1].name.clone(),
                expected: format!("{DIGEST_PREFIX}{}", "b".repeat(64)),
                actual: format!("{DIGEST_PREFIX}{}", "c".repeat(64)),
            },
        ];

        let written =
            record_digests(dir.path(), &committed, &statuses).expect("the write succeeds");
        assert_eq!(written, vec![committed[0].id.clone()]);

        let reloaded = deacon_conformance::discovery::queue::DiscoveryData::load(dir.path())
            .expect("the manifest reloads");
        assert_eq!(
            reloaded.corpus[0].content_digest.as_deref(),
            Some(digest.as_str())
        );
        assert_eq!(reloaded.corpus[1].content_digest, None);
        assert!(
            corpus::check(&reloaded.corpus).is_empty(),
            "a recorded digest must leave the manifest D4-clean"
        );
    }

    #[test]
    fn nothing_to_record_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let committed = vec![entry("src/go", None)];
        // Deliberately does NOT seed the file: a run that recorded nothing must not
        // create or rewrite the manifest at all.
        let written = record_digests(dir.path(), &committed, &[]).expect("no-op");
        assert!(written.is_empty());
        assert!(
            !deacon_conformance::discovery::queue::corpus_path(dir.path()).exists(),
            "a no-op must not touch the manifest"
        );
    }
}
