//! The control manifest: an operator-supplied list of Features to refuse or warn about.
//!
//! The reference CLI fetches one of these from
//! `https://containers.dev/static/devcontainer-control-manifest.json` on every
//! `up` and `build`, and uses it to refuse builds (`disallowedFeatures`) and to
//! warn about known-bad versions (`featureAdvisories`).
//!
//! **deacon implements the mechanism and not the default.** The format, the
//! matching rule and the version arithmetic here are the reference's, so
//! pointing `--control-manifest` at the reference's own URL reproduces its
//! behavior exactly. But deacon consults nothing unless an operator names a
//! source, for reasons that are worth writing down because they are not
//! obvious from the reference's code:
//!
//! - The capability was proposed to the spec as [devcontainers/spec#226] in
//!   April 2023 and **closed by its own author** in June 2023 — *"Out of scope
//!   for the spec for now."* It shipped in the CLI regardless. So this is not a
//!   spec-silent surface where the reference is the authority by default; it is
//!   a surface the spec explicitly declined to standardize.
//! - The reference's list is not version-controlled. It lives as a single
//!   mutable `latest` tag on `ghcr.io/devcontainers/control-manifest`, which a
//!   daily cron `oras pull`s into the containers.dev site build. There is no
//!   public source, no pull request, no review and no retained history —
//!   whoever can push that package can stop other people's containers, and
//!   nothing public records that it changed.
//! - Every entry on it today is a test fixture. Two of the referenced Features
//!   do not exist and their documentation URLs 404.
//!
//! Naming a source is therefore an explicit, per-operator decision. The most
//! useful shape is not the reference's URL at all but an organization's own
//! list, served from somewhere its own people can review.
//!
//! One deliberate behavioral divergence, on the failure mode: when a fetch
//! fails the reference falls back to an EMPTY manifest, which silently disables
//! the very gate the operator asked for. deacon refuses instead — see
//! [`load`]. A stale cached copy is used, loudly, in preference to failing; an
//! empty one never is.
//!
//! [devcontainers/spec#226]: https://github.com/devcontainers/spec/issues/226

use crate::errors::{DeaconError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, info, warn};

/// How long a cached fetch stays fresh before deacon re-fetches. The
/// reference's own window, and for the same reason: a manifest is consulted on
/// every `up`/`build`, and re-fetching per invocation would hammer the server
/// without telling anyone anything new.
const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

/// Characters that terminate the prefix half of a Feature id.
///
/// A tag (`:`), a digest (`@`) or a deeper path segment (`/`) continues the
/// same Feature, so a prefix covers them; any other character starts a
/// DIFFERENT Feature and must not match. This is the rule spec#226 specified
/// in as many words — "This allows for matching, e.g.,
/// `ghcr.io/devcontainers/features/node` while not matching
/// `ghcr.io/devcontainers/features/nodejs`" — and the rule the reference's
/// `findDisallowedFeatureEntry` implements.
const ID_SEPARATORS: [u8; 3] = *b"/:@";

/// Whether `prefix` covers `feature_id` under the Feature-id prefix rule.
///
/// ```
/// use deacon_core::control_manifest::feature_id_covered_by;
///
/// assert!(feature_id_covered_by("example.io/test/node", "example.io/test/node"));
/// assert!(feature_id_covered_by("example.io/test/node", "example.io/test/node:1"));
/// assert!(!feature_id_covered_by("example.io/test/node", "example.io/test/nodejs"));
/// ```
pub fn feature_id_covered_by(prefix: &str, feature_id: &str) -> bool {
    // An empty prefix is a prefix of everything. The reference can only get one
    // from a malformed manifest and would then block every Feature; deacon
    // drops it, here and in the sanitizer, because "block the world" is never
    // what a blank line meant.
    if prefix.is_empty() {
        return false;
    }
    let Some(rest) = feature_id.strip_prefix(prefix) else {
        return false;
    };
    // `strip_prefix` succeeded, so the boundary is a char boundary and the
    // separators are ASCII — a byte look is exact here.
    match rest.as_bytes().first() {
        None => true, // Feature id equal to the prefix.
        Some(byte) => ID_SEPARATORS.contains(byte),
    }
}

/// Parse a version into its numeric components, the reference's way
/// (`parseVersion` in `spec-common/commonUtils.ts`).
///
/// Deliberately NOT semver: it takes the leading dotted-integer run and ignores
/// everything after it, accepts any number of components, and has no concept of
/// prerelease ordering. `1.2` and `1.2.0` therefore compare equal, which is the
/// behavior advisory ranges are written against. Using
/// [`crate::semver_utils::parse_version`] here would quietly change which
/// versions an advisory covers.
fn parse_version_components(value: &str) -> Option<Vec<u64>> {
    let trimmed = value.trim().trim_start_matches('\'');
    let digits = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let leading: String = digits
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let leading = leading.trim_end_matches('.');
    if leading.is_empty() {
        return None;
    }
    leading
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

/// Component-wise "strictly earlier", missing components read as zero — the
/// reference's `isEarlierVersion`.
fn is_earlier(left: &[u64], right: &[u64]) -> bool {
    for index in 0..left.len().max(right.len()) {
        let l = left.get(index).copied().unwrap_or(0);
        let r = right.get(index).copied().unwrap_or(0);
        if l != r {
            return l < r;
        }
    }
    false // Equal.
}

/// A Feature the manifest refuses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisallowedFeature {
    /// Matched against configured Feature ids by [`feature_id_covered_by`].
    pub feature_id_prefix: String,
    /// Optional page explaining the refusal, surfaced in the diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

/// A known problem with a version RANGE of a Feature: `[introduced, fixed)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureAdvisory {
    /// Registry-qualified Feature id WITHOUT a version (`<registry>/<path>`).
    pub feature_id: String,
    /// First affected version, inclusive.
    pub introduced_in_version: String,
    /// First fixed version, exclusive.
    pub fixed_in_version: String,
    /// Human-readable summary, printed verbatim.
    pub description: String,
    /// Optional page with the details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

/// A parsed, sanitized control manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlManifest {
    /// Features to refuse outright.
    #[serde(default)]
    pub disallowed_features: Vec<DisallowedFeature>,
    /// Version ranges to warn about.
    #[serde(default)]
    pub feature_advisories: Vec<FeatureAdvisory>,
}

impl ControlManifest {
    /// Parse manifest bytes, dropping malformed entries rather than failing.
    ///
    /// This leniency is the reference's `sanitizeControlManifest` and is the
    /// right call for a document deacon does not author: a single future entry
    /// shape must not disable the entries deacon does understand. It is
    /// deliberately NOT the strictness deacon applies to a user's own
    /// `devcontainer.json` (constitution IV) — that document is the
    /// developer's mistake to hear about; this one is somebody else's file.
    ///
    /// Bytes that are not JSON at all ARE an error: that is a broken source,
    /// not an unrecognized entry.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
            DeaconError::Config(crate::errors::ConfigError::Parsing {
                message: format!("control manifest is not valid JSON: {e}"),
            })
        })?;
        Ok(Self::sanitize(&raw))
    }

    /// Keep only the entries whose required fields are present, well-typed and
    /// non-empty.
    fn sanitize(raw: &serde_json::Value) -> Self {
        let string_at = |value: &serde_json::Value, key: &str| -> Option<String> {
            value.get(key)?.as_str().map(str::to_string)
        };
        let entries = |key: &str| -> Vec<serde_json::Value> {
            raw.get(key)
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
        };

        let disallowed_features = entries("disallowedFeatures")
            .iter()
            .filter_map(|entry| {
                let feature_id_prefix = string_at(entry, "featureIdPrefix")?;
                // An empty prefix would match every Feature; see
                // `feature_id_covered_by`.
                if feature_id_prefix.is_empty() {
                    warn!("control manifest: dropping a disallowed entry with an empty featureIdPrefix");
                    return None;
                }
                Some(DisallowedFeature {
                    feature_id_prefix,
                    documentation_url: string_at(entry, "documentationURL"),
                })
            })
            .collect();

        let feature_advisories = entries("featureAdvisories")
            .iter()
            .filter_map(|entry| {
                Some(FeatureAdvisory {
                    feature_id: string_at(entry, "featureId")?,
                    introduced_in_version: string_at(entry, "introducedInVersion")?,
                    fixed_in_version: string_at(entry, "fixedInVersion")?,
                    description: string_at(entry, "description")?,
                    documentation_url: string_at(entry, "documentationURL"),
                })
            })
            .collect();

        Self {
            disallowed_features,
            feature_advisories,
        }
    }

    /// Whether the manifest says nothing at all.
    pub fn is_empty(&self) -> bool {
        self.disallowed_features.is_empty() && self.feature_advisories.is_empty()
    }

    /// The first entry covering `feature_id`, if any.
    pub fn disallowed_entry_for(&self, feature_id: &str) -> Option<&DisallowedFeature> {
        self.disallowed_features
            .iter()
            .find(|entry| feature_id_covered_by(&entry.feature_id_prefix, feature_id))
    }

    /// The advisories affecting `version` of `feature_id`.
    ///
    /// `feature_id` is the registry-qualified id without a version; `version`
    /// is the resolved version actually being installed. An advisory applies
    /// when `introduced <= version < fixed`. An unparseable version on either
    /// side excludes the advisory rather than including it — a warning nobody
    /// can act on is worse than silence.
    pub fn advisories_for(&self, feature_id: &str, version: &str) -> Vec<&FeatureAdvisory> {
        let Some(actual) = parse_version_components(version) else {
            warn!(
                feature_id,
                version, "unable to parse the Feature version; skipping advisory matching"
            );
            return Vec::new();
        };
        self.feature_advisories
            .iter()
            .filter(|advisory| advisory.feature_id == feature_id)
            .filter(|advisory| {
                let (Some(introduced), Some(fixed)) = (
                    parse_version_components(&advisory.introduced_in_version),
                    parse_version_components(&advisory.fixed_in_version),
                ) else {
                    return false;
                };
                !is_earlier(&actual, &introduced) && is_earlier(&actual, &fixed)
            })
            .collect()
    }
}

/// Where a control manifest comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlManifestSource {
    /// Fetched over HTTP(S) and cached.
    Url(String),
    /// Read from the local filesystem on every use, never cached.
    File(PathBuf),
}

impl ControlManifestSource {
    /// Interpret an operator-supplied `--control-manifest` value.
    ///
    /// An `http://` or `https://` value is a URL; anything else is a path. A
    /// local file is the shape that makes this testable and the shape an
    /// organization checking its list into a repository actually wants.
    pub fn parse(value: &str) -> Self {
        if value.starts_with("http://") || value.starts_with("https://") {
            ControlManifestSource::Url(value.to_string())
        } else {
            ControlManifestSource::File(PathBuf::from(value))
        }
    }
}

impl std::fmt::Display for ControlManifestSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlManifestSource::Url(url) => write!(f, "{url}"),
            ControlManifestSource::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// The cache file a URL source reads and writes.
fn cache_path_for(cache_dir: &Path, url: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(url.as_bytes());
    // Keyed by URL so two configured sources never overwrite each other.
    cache_dir
        .join("control-manifest")
        .join(format!("{:x}.json", digest))
}

/// Write `bytes` to `path` atomically.
///
/// A plain write truncates then streams, so a shorter payload over a longer
/// file leaves trailing bytes and the next read fails to parse — the same
/// hazard the disk cache's index has.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "cache path has no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("cache"),
        std::process::id()
    ));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

/// Load the manifest an operator named.
///
/// A [`ControlManifestSource::File`] is read every time and errors are fatal:
/// the operator named a path, so an unreadable one is a real mistake and
/// proceeding without the policy would defeat it.
///
/// A [`ControlManifestSource::Url`] is served from cache while the cached copy
/// is younger than [`CACHE_TTL`], and fetched otherwise. **On a fetch failure
/// deacon does not fall back to an empty manifest.** The reference does, which
/// means a network blip silently turns its gate off; since deacon's gate exists
/// only because an operator asked for it, turning it off silently is the one
/// outcome nobody wants. A stale cached copy is used instead, with a warning
/// naming the staleness; with no cached copy at all, the load fails.
pub async fn load(source: &ControlManifestSource, cache_dir: &Path) -> Result<ControlManifest> {
    match source {
        ControlManifestSource::File(path) => {
            let bytes = std::fs::read(path).map_err(|e| {
                DeaconError::Config(crate::errors::ConfigError::Validation {
                    message: format!(
                        "control manifest '{}' could not be read: {e}",
                        path.display()
                    ),
                })
            })?;
            debug!(path = %path.display(), "loaded control manifest from file");
            ControlManifest::parse(&bytes)
        }
        ControlManifestSource::Url(url) => load_from_url(url, cache_dir).await,
    }
}

async fn load_from_url(url: &str, cache_dir: &Path) -> Result<ControlManifest> {
    let cache_file = cache_path_for(cache_dir, url);
    let cached = std::fs::read(&cache_file).ok();
    let cache_age = std::fs::metadata(&cache_file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok());

    if let (Some(bytes), Some(age)) = (cached.as_ref(), cache_age) {
        if age < CACHE_TTL {
            debug!(url, age_secs = age.as_secs(), "control manifest cache hit");
            return ControlManifest::parse(bytes);
        }
    }

    match fetch(url).await {
        Ok(bytes) => {
            // Parse BEFORE caching, so a broken response never becomes the
            // stale copy a later failure falls back to.
            let manifest = ControlManifest::parse(&bytes)?;
            if let Err(e) = write_atomically(&cache_file, &bytes) {
                // The manifest is in hand; failing the run over the cache would
                // be worse than re-fetching next time.
                warn!(url, error = %e, "could not cache the control manifest");
            }
            Ok(manifest)
        }
        Err(fetch_error) => match cached {
            Some(bytes) => {
                let age = cache_age
                    .map(|a| format!("{} minutes", a.as_secs() / 60))
                    .unwrap_or_else(|| "unknown".to_string());
                warn!(
                    url,
                    error = %fetch_error,
                    cached_age = %age,
                    "could not refresh the control manifest; using the cached copy"
                );
                ControlManifest::parse(&bytes)
            }
            None => Err(DeaconError::Network {
                message: format!(
                    "control manifest '{url}' could not be fetched and no cached copy exists: {fetch_error}. \
                     deacon refuses rather than proceeding with no policy — pass a reachable \
                     --control-manifest, or unset it to disable the check."
                ),
            }),
        },
    }
}

async fn fetch(url: &str) -> std::result::Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        // Deliberately NOT the reference's `devcontainers-vscode`: deacon is
        // not that client, and a server that wants to tell its callers apart
        // should be able to.
        .header("user-agent", concat!("deacon/", env!("CARGO_PKG_VERSION")))
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

/// Format the advisories for a set of Features as a single warning block.
///
/// Returns `None` when nothing is affected, so the caller emits nothing at all
/// rather than an empty banner. `features` is `(id, version)` pairs for the OCI
/// Features actually being installed.
pub fn format_advisories(
    manifest: &ControlManifest,
    features: &[(String, String)],
) -> Option<String> {
    let mut affected: Vec<(&str, &str, Vec<&FeatureAdvisory>)> = features
        .iter()
        .map(|(id, version)| {
            (
                id.as_str(),
                version.as_str(),
                manifest.advisories_for(id, version),
            )
        })
        .filter(|(_, _, advisories)| !advisories.is_empty())
        .collect();
    if affected.is_empty() {
        return None;
    }
    affected.sort_by(|a, b| a.0.cmp(b.0));

    let mut out = String::from("FEATURE ADVISORIES:");
    for (id, version, advisories) in affected {
        out.push_str(&format!("\n- {id}:{version}:"));
        for advisory in advisories {
            out.push_str(&format!(
                "\n  - {} (introduced in {}, fixed in {}{})",
                advisory.description,
                advisory.introduced_in_version,
                advisory.fixed_in_version,
                advisory
                    .documentation_url
                    .as_ref()
                    .map(|u| format!(", see {u}"))
                    .unwrap_or_default(),
            ));
        }
    }
    out.push_str(
        "\nIt is recommended that you update your configuration to versions of these Features with the fixes applied.",
    );
    Some(out)
}

/// Emit the advisory block, if any, at warning level.
pub fn log_advisories(manifest: &ControlManifest, features: &[(String, String)]) {
    if let Some(block) = format_advisories(manifest, features) {
        warn!("{block}");
    } else {
        info!("no Feature advisories apply to this configuration");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Loopback, port 1: refuses immediately, reaches no network.
    const UNREACHABLE: &str = "http://127.0.0.1:1/manifest.json";

    const REFERENCE_SHAPED: &str = r#"{
        "disallowedFeatures": [
            { "featureIdPrefix": "ghcr.io/devcontainers/features/disallowed-feature",
              "documentationURL": "https://example.invalid/why" }
        ],
        "featureAdvisories": [
            { "featureId": "ghcr.io/devcontainers/features/feature-with-advisory",
              "introducedInVersion": "1.0.7",
              "fixedInVersion": "1.1.10",
              "description": "Feature with advisory for testing.",
              "documentationURL": "https://example.invalid/advisory" }
        ]
    }"#;

    #[test]
    fn a_reference_shaped_manifest_round_trips() {
        let manifest = ControlManifest::parse(REFERENCE_SHAPED.as_bytes()).unwrap();
        assert_eq!(manifest.disallowed_features.len(), 1);
        assert_eq!(manifest.feature_advisories.len(), 1);
        assert_eq!(
            manifest.disallowed_features[0].documentation_url.as_deref(),
            Some("https://example.invalid/why")
        );
    }

    #[test]
    fn the_prefix_rule_is_the_one_spec_226_specified() {
        // The vectors the reference's own unit test pins.
        let p = "example.io/test/node";
        assert!(feature_id_covered_by(p, "example.io/test/node"));
        assert!(feature_id_covered_by(p, "example.io/test/node:1"));
        assert!(feature_id_covered_by(p, "example.io/test/node/js"));
        assert!(feature_id_covered_by(p, "example.io/test/node@abc"));
        assert!(!feature_id_covered_by(p, "example.io/test/nodej"));
        assert!(!feature_id_covered_by(p, "example.io/test/nod"));
        assert!(!feature_id_covered_by(p, "example.io/test/node.js"));
    }

    #[test]
    fn an_empty_prefix_is_dropped_rather_than_matching_everything() {
        assert!(!feature_id_covered_by("", "anything"));
        let manifest = ControlManifest::parse(
            br#"{"disallowedFeatures":[{"featureIdPrefix":""},{"featureIdPrefix":"a/b"}]}"#,
        )
        .unwrap();
        assert_eq!(manifest.disallowed_features.len(), 1);
        assert_eq!(manifest.disallowed_features[0].feature_id_prefix, "a/b");
    }

    #[test]
    fn malformed_entries_are_dropped_and_the_rest_survive() {
        // The reference's leniency: one unusable entry must not disable the
        // entries deacon does understand.
        let manifest = ControlManifest::parse(
            br#"{
                "disallowedFeatures": [
                    { "notAPrefix": "x" },
                    { "featureIdPrefix": 42 },
                    { "featureIdPrefix": "good/one" }
                ],
                "featureAdvisories": [
                    { "featureId": "a/b", "introducedInVersion": "1.0.0" },
                    { "featureId": "a/b", "introducedInVersion": "1.0.0",
                      "fixedInVersion": "2.0.0", "description": "ok" }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(manifest.disallowed_features.len(), 1);
        assert_eq!(manifest.feature_advisories.len(), 1);
    }

    #[test]
    fn bytes_that_are_not_json_are_an_error_not_an_empty_manifest() {
        // A broken source must be audible. Returning an empty manifest here is
        // exactly the silent-disable this module exists to avoid.
        assert!(ControlManifest::parse(b"<html>404</html>").is_err());
    }

    #[test]
    fn an_absent_key_is_simply_empty() {
        let manifest = ControlManifest::parse(b"{}").unwrap();
        assert!(manifest.is_empty());
    }

    #[test]
    fn versions_compare_component_wise_with_missing_parts_as_zero() {
        // NOT semver: `1.2` and `1.2.0` are the same version here.
        assert_eq!(parse_version_components("1.2"), Some(vec![1, 2]));
        assert_eq!(parse_version_components("v1.2.3"), Some(vec![1, 2, 3]));
        assert_eq!(
            parse_version_components("1.2.3-beta.1"),
            Some(vec![1, 2, 3])
        );
        assert_eq!(parse_version_components("latest"), None);

        assert!(is_earlier(&[1, 2], &[1, 2, 1]));
        assert!(!is_earlier(&[1, 2], &[1, 2, 0]));
        assert!(!is_earlier(&[1, 2, 0], &[1, 2]));
        assert!(is_earlier(&[1, 9], &[1, 10]));
    }

    #[test]
    fn an_advisory_covers_its_half_open_range() {
        let manifest = ControlManifest::parse(REFERENCE_SHAPED.as_bytes()).unwrap();
        let id = "ghcr.io/devcontainers/features/feature-with-advisory";

        assert!(manifest.advisories_for(id, "1.0.6").is_empty(), "before");
        assert_eq!(
            manifest.advisories_for(id, "1.0.7").len(),
            1,
            "introduced is INCLUSIVE"
        );
        assert_eq!(manifest.advisories_for(id, "1.1.9").len(), 1, "inside");
        assert!(
            manifest.advisories_for(id, "1.1.10").is_empty(),
            "fixed is EXCLUSIVE"
        );
        assert!(manifest.advisories_for(id, "2.0.0").is_empty(), "after");
        assert!(
            manifest.advisories_for("other/feature", "1.0.8").is_empty(),
            "a different Feature is untouched"
        );
    }

    #[test]
    fn an_unparseable_version_matches_no_advisory() {
        let manifest = ControlManifest::parse(REFERENCE_SHAPED.as_bytes()).unwrap();
        assert!(
            manifest
                .advisories_for(
                    "ghcr.io/devcontainers/features/feature-with-advisory",
                    "latest"
                )
                .is_empty()
        );
    }

    #[test]
    fn the_advisory_block_names_every_affected_feature_and_nothing_else() {
        let manifest = ControlManifest::parse(REFERENCE_SHAPED.as_bytes()).unwrap();
        let affected = vec![
            (
                "ghcr.io/devcontainers/features/feature-with-advisory".to_string(),
                "1.0.8".to_string(),
            ),
            ("ghcr.io/other/feature".to_string(), "1.0.0".to_string()),
        ];
        let block = format_advisories(&manifest, &affected).expect("one Feature is affected");
        assert!(block.contains("feature-with-advisory:1.0.8"));
        assert!(block.contains("Feature with advisory for testing."));
        assert!(block.contains("introduced in 1.0.7, fixed in 1.1.10"));
        assert!(block.contains("https://example.invalid/advisory"));
        assert!(
            !block.contains("ghcr.io/other/feature"),
            "an unaffected Feature must not appear"
        );

        assert!(
            format_advisories(
                &manifest,
                &[("ghcr.io/other/feature".into(), "1.0.0".into())]
            )
            .is_none(),
            "nothing affected means no banner at all"
        );
    }

    #[test]
    fn a_source_is_a_url_only_when_it_looks_like_one() {
        assert_eq!(
            ControlManifestSource::parse("https://example.invalid/m.json"),
            ControlManifestSource::Url("https://example.invalid/m.json".into())
        );
        assert_eq!(
            ControlManifestSource::parse("/etc/deacon/manifest.json"),
            ControlManifestSource::File(PathBuf::from("/etc/deacon/manifest.json"))
        );
        assert_eq!(
            ControlManifestSource::parse("./manifest.json"),
            ControlManifestSource::File(PathBuf::from("./manifest.json"))
        );
    }

    #[tokio::test]
    async fn a_file_source_is_read_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        std::fs::write(&path, REFERENCE_SHAPED).unwrap();

        let manifest = load(
            &ControlManifestSource::File(path),
            &dir.path().join("cache"),
        )
        .await
        .unwrap();
        assert_eq!(manifest.disallowed_features.len(), 1);
    }

    #[tokio::test]
    async fn an_unreadable_file_source_is_an_error_not_an_empty_policy() {
        let dir = tempfile::tempdir().unwrap();
        let err = load(
            &ControlManifestSource::File(dir.path().join("absent.json")),
            &dir.path().join("cache"),
        )
        .await
        .expect_err("a named-but-missing manifest must fail");
        assert!(err.to_string().contains("could not be read"));
    }

    #[tokio::test]
    async fn an_unreachable_url_with_no_cache_refuses_rather_than_proceeding() {
        // The deliberate divergence: the reference would return an empty
        // manifest here and silently disable the operator's policy.
        let dir = tempfile::tempdir().unwrap();
        let err = load(
            // Loopback to a port nothing can be bound to without root, so
            // this refuses instantly instead of burning a connect timeout —
            // and it leaves the host, and any sandbox, entirely alone.
            &ControlManifestSource::Url(UNREACHABLE.into()),
            dir.path(),
        )
        .await
        .expect_err("an unreachable source with no cache must fail");
        let rendered = err.to_string();
        assert!(rendered.contains("no cached copy exists"), "{rendered}");
    }

    #[tokio::test]
    async fn an_unreachable_url_falls_back_to_the_cached_copy() {
        let dir = tempfile::tempdir().unwrap();
        let url = UNREACHABLE;
        let cache_file = cache_path_for(dir.path(), url);
        write_atomically(&cache_file, REFERENCE_SHAPED.as_bytes()).unwrap();
        // Age it past the TTL so the loader must try the network and fail.
        let stale = SystemTime::now() - CACHE_TTL - Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&cache_file)
            .unwrap()
            .set_modified(stale)
            .unwrap();

        let manifest = load(&ControlManifestSource::Url(url.into()), dir.path())
            .await
            .expect("a stale cached copy beats failing");
        assert_eq!(manifest.disallowed_features.len(), 1);
    }

    #[tokio::test]
    async fn a_fresh_cache_is_served_without_touching_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let url = UNREACHABLE;
        write_atomically(
            &cache_path_for(dir.path(), url),
            REFERENCE_SHAPED.as_bytes(),
        )
        .unwrap();

        // The URL is unroutable, so a network attempt would take the connect
        // timeout and then fail; returning promptly proves the cache was used.
        let manifest = load(&ControlManifestSource::Url(url.into()), dir.path())
            .await
            .expect("a fresh cache must be served directly");
        assert_eq!(manifest.disallowed_features.len(), 1);
    }

    #[test]
    fn two_urls_never_share_a_cache_file() {
        let dir = Path::new("/cache");
        assert_ne!(
            cache_path_for(dir, "https://a.invalid/m.json"),
            cache_path_for(dir, "https://b.invalid/m.json")
        );
    }
}
