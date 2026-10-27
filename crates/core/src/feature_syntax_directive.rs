//! The `# syntax=` parser directive heading the Dockerfile deacon generates to
//! install Features — the reference's `dockerfilePrefixContent` (#732).
//!
//! The reference decides it in `addContainerFeatures`
//! (`src/spec-node/containerFeatures.ts` at the pinned oracle; compiled into
//! `dist/spec-node/devContainersSpecCLI.js` as function `Wj`):
//!
//! ```text
//! const useBuildKitBuildContexts = buildKitVersion ? !isEarlierVersion(buildKitVersion, [0, 8, 0]) : false;
//! const skipDefaultSyntax = dockerEngineVersion ? !isEarlierVersion(dockerEngineVersion, [23, 0, 0]) : false;
//! const syntax = imageBuildInfo.dockerfile?.preamble.directives.syntax;
//! const dockerfilePrefixContent = `${
//!   omitSyntaxDirective ? ''
//!   : skipDefaultSyntax ? (syntax ? `# syntax=${syntax}` : '')
//!   : useBuildKitBuildContexts && !(imageBuildInfo.dockerfile && supportsBuildContexts(imageBuildInfo.dockerfile)) ? '# syntax=docker/dockerfile:1.4'
//!   : syntax ? `# syntax=${syntax}` : ''}
//! ARG _DEV_CONTAINERS_BASE_IMAGE=scratch
//! `;
//! ```
//!
//! Its callers (`buildAndExtendImage` for a Dockerfile configuration, the Compose
//! resolver for a `build:` service) then strip the user's own leading directive
//! from the base document with `/^\s*#\s*syntax\s*=.*[\r\n]/` and write
//! `${dockerfilePrefixContent}${base}\n${featureStage}`, so the prefix REPLACES the
//! user's directive rather than adding a second one.
//!
//! Three details are easy to get wrong and are each pinned by a test below:
//!
//! - A version that could not be read is not a guess. `docker version` /
//!   `docker buildx version` failing, or printing nothing that contains
//!   `MAJOR.MINOR.PATCH`, leaves the reference's version `undefined`, and both
//!   predicates are then `false`: an unknown engine is treated as OLD (inject if
//!   BuildKit is known to be new enough), an unknown BuildKit as unable to use
//!   build contexts (never inject). Those are the reference's answers, so they
//!   are this module's.
//! - `supportsBuildContexts` is three-valued and the third value is the string
//!   `'unknown'` — which JavaScript treats as TRUTHY. A `syntax=` naming some
//!   other frontend image therefore does NOT get the 1.4 directive forced over it;
//!   the user's directive is kept.
//! - With no base Dockerfile at all (an image-reference configuration), the
//!   `!(dockerfile && …)` guard is `true`, so the injection fires exactly as for a
//!   Dockerfile without a directive.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::dockerfile_utils::{BuildContextSupport, Dockerfile, supports_build_contexts};

/// The Docker engine version from which the reference stops injecting its
/// default frontend (`minDockerEngineVersion` in `containerFeatures.ts`). Engine
/// 23.0 made BuildKit the default builder with a bundled frontend that supports
/// named build contexts.
pub const SKIP_DEFAULT_SYNTAX_MIN_ENGINE: [u64; 3] = [23, 0, 0];

/// The BuildKit (`buildx`) version from which the reference uses named build
/// contexts at all.
pub const BUILD_CONTEXTS_MIN_BUILDKIT: [u64; 3] = [0, 8, 0];

/// The frontend the reference injects when the build needs build contexts and
/// nothing guarantees the frontend has them.
pub const DEFAULT_SYNTAX: &str = "docker/dockerfile:1.4";

/// `(?<major>[0-9]+)\.(?<minor>[0-9]+)\.(?<patch>[0-9]+)` — the reference's
/// `versionMatch`, read out of the raw `version` command output.
static VERSION_TRIPLET: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"([0-9]+)\.([0-9]+)\.([0-9]+)").expect("version triplet regex must compile")
});

/// `/^\s*#\s*syntax\s*=.*[\r\n]/` without the `m` flag: anchored at the very start
/// of the document. JavaScript's `.` stops at `\n`, `\r`, U+2028 and U+2029, so it
/// is spelled out rather than left to the `regex` crate's `.` (which stops only at
/// `\n`).
static LEADING_SYNTAX_DIRECTIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*#\s*syntax\s*=[^\n\r\u{2028}\u{2029}]*[\r\n]")
        .expect("leading syntax directive regex must compile")
});

/// The first `MAJOR.MINOR.PATCH` in a version command's output — the reference's
/// `versionMatch` passed through `parseVersion`.
///
/// `None` when the output carries no such triplet, which the reference also
/// treats as an unknown version. A component too large for `u64` is likewise
/// unknown; no real engine prints one.
pub fn version_triplet(output: &str) -> Option<[u64; 3]> {
    let caps = VERSION_TRIPLET.captures(output)?;
    let part = |i: usize| caps.get(i)?.as_str().parse::<u64>().ok();
    Some([part(1)?, part(2)?, part(3)?])
}

/// Everything the reference's prefix decision reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyntaxDirectiveInputs<'a> {
    /// `--omit-syntax-directive`.
    pub omit_syntax_directive: bool,
    /// The Docker engine version (`docker version --format {{.Server.Version}}`),
    /// `None` when it could not be read.
    pub docker_engine_version: Option<[u64; 3]>,
    /// The BuildKit version (`docker buildx version`), `None` when it could not be
    /// read OR when this build does not use BuildKit build contexts at all — the
    /// reference clears it for `--buildkit never` and for Compose builds that
    /// cannot take `additional_contexts`.
    pub buildkit_version: Option<[u64; 3]>,
    /// The user's base Dockerfile, parsed. `None` for an image-reference
    /// configuration, which has no Dockerfile of its own.
    pub base_dockerfile: Option<&'a Dockerfile>,
}

/// The `# syntax=` line to head the generated Feature Dockerfile with, or `None`
/// for no directive. The reference's four-way `dockerfilePrefixContent`; see the
/// module documentation.
pub fn feature_dockerfile_syntax_directive(inputs: SyntaxDirectiveInputs<'_>) -> Option<String> {
    if inputs.omit_syntax_directive {
        return None;
    }

    let user_syntax = inputs
        .base_dockerfile
        .and_then(|d| d.preamble.directives.get("syntax"))
        .map(|syntax| format!("# syntax={syntax}"));

    let skip_default_syntax = inputs
        .docker_engine_version
        .is_some_and(|v| v >= SKIP_DEFAULT_SYNTAX_MIN_ENGINE);
    if skip_default_syntax {
        return user_syntax;
    }

    let use_buildkit_build_contexts = inputs
        .buildkit_version
        .is_some_and(|v| v >= BUILD_CONTEXTS_MIN_BUILDKIT);
    // `supportsBuildContexts` returns `true`, `false` or `'unknown'`, and the
    // caller negates its truthiness: only a definite "no" (or no Dockerfile at all)
    // lets the default frontend in.
    let frontend_lacks_build_contexts = inputs
        .base_dockerfile
        .is_none_or(|d| supports_build_contexts(d) == BuildContextSupport::No);
    if use_buildkit_build_contexts && frontend_lacks_build_contexts {
        return Some(format!("# syntax={DEFAULT_SYNTAX}"));
    }

    user_syntax
}

/// The base document with its own leading `# syntax=` line removed, which is
/// what the reference does before writing its prefix in front of it.
pub fn strip_leading_syntax_directive(content: &str) -> &str {
    match LEADING_SYNTAX_DIRECTIVE.find(content) {
        Some(m) => &content[m.end()..],
        None => content,
    }
}

/// Head `base` with `directive`, replacing any leading `# syntax=` line `base`
/// already carried — the reference's `${dockerfilePrefixContent}${base}`, minus
/// the `ARG _DEV_CONTAINERS_BASE_IMAGE` line deacon's install stage does not use.
pub fn apply_syntax_directive(base: &str, directive: Option<&str>) -> String {
    let body = strip_leading_syntax_directive(base);
    match directive {
        Some(line) => {
            let mut out = String::with_capacity(line.len() + 1 + body.len());
            out.push_str(line);
            out.push('\n');
            out.push_str(body);
            out
        }
        None => body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dockerfile_utils::extract_dockerfile;

    const OLD_ENGINE: Option<[u64; 3]> = Some([20, 10, 24]);
    const NEW_ENGINE: Option<[u64; 3]> = Some([23, 0, 0]);
    const NEW_BUILDKIT: Option<[u64; 3]> = Some([0, 12, 1]);
    const OLD_BUILDKIT: Option<[u64; 3]> = Some([0, 7, 9]);

    fn decide(
        omit: bool,
        engine: Option<[u64; 3]>,
        buildkit: Option<[u64; 3]>,
        dockerfile: Option<&str>,
    ) -> Option<String> {
        let parsed = dockerfile.map(extract_dockerfile);
        feature_dockerfile_syntax_directive(SyntaxDirectiveInputs {
            omit_syntax_directive: omit,
            docker_engine_version: engine,
            buildkit_version: buildkit,
            base_dockerfile: parsed.as_ref(),
        })
    }

    const NO_DIRECTIVE: &str = "FROM debian\n";
    const OLD_FRONTEND: &str = "# syntax=docker/dockerfile:1.2\nFROM debian\n";
    const NEW_FRONTEND: &str = "# syntax=docker/dockerfile:1.5\nFROM debian\n";
    const FOREIGN_FRONTEND: &str = "# syntax=mycompany/frontend:2\nFROM debian\n";

    // Branch 1: `omitSyntaxDirective` wins over everything, including a
    // directive the user wrote and an engine that would inject.
    #[test]
    fn omit_suppresses_every_directive() {
        assert_eq!(decide(true, OLD_ENGINE, NEW_BUILDKIT, None), None);
        assert_eq!(
            decide(true, OLD_ENGINE, NEW_BUILDKIT, Some(NO_DIRECTIVE)),
            None
        );
        assert_eq!(
            decide(true, NEW_ENGINE, NEW_BUILDKIT, Some(NEW_FRONTEND)),
            None
        );
    }

    // Branch 2: an engine >= 23.0.0 never injects; it only carries the user's own.
    #[test]
    fn modern_engine_keeps_only_the_users_directive() {
        assert_eq!(decide(false, NEW_ENGINE, NEW_BUILDKIT, None), None);
        assert_eq!(
            decide(false, NEW_ENGINE, NEW_BUILDKIT, Some(NO_DIRECTIVE)),
            None
        );
        assert_eq!(
            decide(false, NEW_ENGINE, NEW_BUILDKIT, Some(OLD_FRONTEND)),
            Some("# syntax=docker/dockerfile:1.2".to_string())
        );
        assert_eq!(
            decide(false, Some([29, 7, 2]), NEW_BUILDKIT, Some(OLD_FRONTEND)),
            Some("# syntax=docker/dockerfile:1.2".to_string())
        );
    }

    // Branch 3: an old engine with BuildKit >= 0.8 injects 1.4 whenever the
    // frontend is not known to support build contexts — including no Dockerfile.
    #[test]
    fn old_engine_with_buildkit_injects_the_default_frontend() {
        let injected = Some("# syntax=docker/dockerfile:1.4".to_string());
        assert_eq!(decide(false, OLD_ENGINE, NEW_BUILDKIT, None), injected);
        assert_eq!(
            decide(false, OLD_ENGINE, NEW_BUILDKIT, Some(NO_DIRECTIVE)),
            injected
        );
        // The user's 1.2 is REPLACED, which is the point: it cannot read contexts.
        assert_eq!(
            decide(false, OLD_ENGINE, NEW_BUILDKIT, Some(OLD_FRONTEND)),
            injected
        );
        assert_eq!(
            decide(
                false,
                Some([22, 99, 99]),
                Some([0, 8, 0]),
                Some(NO_DIRECTIVE)
            ),
            injected
        );
    }

    // Branch 3 not taken because the frontend already supports contexts, or
    // might (`'unknown'` is truthy): branch 4 keeps the user's directive.
    #[test]
    fn old_engine_keeps_a_frontend_that_supports_or_may_support_contexts() {
        assert_eq!(
            decide(false, OLD_ENGINE, NEW_BUILDKIT, Some(NEW_FRONTEND)),
            Some("# syntax=docker/dockerfile:1.5".to_string())
        );
        assert_eq!(
            decide(false, OLD_ENGINE, NEW_BUILDKIT, Some(FOREIGN_FRONTEND)),
            Some("# syntax=mycompany/frontend:2".to_string())
        );
    }

    // Branch 4 via BuildKit: below 0.8 (or unknown, or not using contexts) the
    // reference never injects, and passes the user's directive through.
    #[test]
    fn old_engine_without_usable_buildkit_keeps_only_the_users_directive() {
        for buildkit in [OLD_BUILDKIT, None] {
            assert_eq!(decide(false, OLD_ENGINE, buildkit, None), None);
            assert_eq!(
                decide(false, OLD_ENGINE, buildkit, Some(NO_DIRECTIVE)),
                None
            );
            assert_eq!(
                decide(false, OLD_ENGINE, buildkit, Some(OLD_FRONTEND)),
                Some("# syntax=docker/dockerfile:1.2".to_string())
            );
        }
    }

    // An unreadable engine version is the reference's `undefined`:
    // `skipDefaultSyntax` is false, so it behaves as an OLD engine.
    #[test]
    fn unknown_engine_version_behaves_as_an_old_engine() {
        assert_eq!(
            decide(false, None, NEW_BUILDKIT, Some(NO_DIRECTIVE)),
            Some("# syntax=docker/dockerfile:1.4".to_string())
        );
        assert_eq!(decide(false, None, None, Some(NO_DIRECTIVE)), None);
    }

    #[test]
    fn version_triplet_reads_what_the_reference_reads() {
        assert_eq!(version_triplet("29.7.2\n"), Some([29, 7, 2]));
        assert_eq!(version_triplet("20.10.24"), Some([20, 10, 24]));
        assert_eq!(
            version_triplet("github.com/docker/buildx v0.12.1-desktop.4 30feaa1\n"),
            Some([0, 12, 1])
        );
        assert_eq!(version_triplet("buildah 1.33.7"), Some([1, 33, 7]));
        // Only two components: the reference's `versionMatch` is absent.
        assert_eq!(version_triplet("24.0"), None);
        assert_eq!(version_triplet(""), None);
        assert_eq!(version_triplet("99999999999999999999999.0.0"), None);
    }

    #[test]
    fn thresholds_compare_component_wise() {
        // 22.99.99 < 23.0.0 even though 99 > 0: whole-version, not per-field.
        assert_eq!(
            decide(false, Some([22, 99, 99]), NEW_BUILDKIT, None),
            Some("# syntax=docker/dockerfile:1.4".to_string())
        );
        assert_eq!(decide(false, Some([100, 0, 0]), NEW_BUILDKIT, None), None);
        assert_eq!(decide(false, OLD_ENGINE, Some([0, 7, 99]), None), None);
        assert_eq!(
            decide(false, OLD_ENGINE, Some([1, 0, 0]), None),
            Some("# syntax=docker/dockerfile:1.4".to_string())
        );
    }

    #[test]
    fn strip_removes_only_a_leading_directive() {
        assert_eq!(
            strip_leading_syntax_directive(OLD_FRONTEND),
            "FROM debian\n"
        );
        assert_eq!(
            strip_leading_syntax_directive("\n  # syntax = foo\r\nFROM debian\n"),
            "\nFROM debian\n"
        );
        assert_eq!(strip_leading_syntax_directive(NO_DIRECTIVE), NO_DIRECTIVE);
        // Not at the head: the reference's regex has no `m` flag.
        let later = "# escape=`\n# syntax=docker/dockerfile:1.5\nFROM debian\n";
        assert_eq!(strip_leading_syntax_directive(later), later);
        // A directive on the last line with no terminator is not matched either.
        assert_eq!(
            strip_leading_syntax_directive("# syntax=docker/dockerfile:1"),
            "# syntax=docker/dockerfile:1"
        );
    }

    #[test]
    fn apply_replaces_the_users_directive_and_never_duplicates_it() {
        assert_eq!(
            apply_syntax_directive(OLD_FRONTEND, Some("# syntax=docker/dockerfile:1.4")),
            "# syntax=docker/dockerfile:1.4\nFROM debian\n"
        );
        assert_eq!(
            apply_syntax_directive(OLD_FRONTEND, Some("# syntax=docker/dockerfile:1.2")),
            OLD_FRONTEND
        );
        assert_eq!(
            apply_syntax_directive(NO_DIRECTIVE, Some("# syntax=docker/dockerfile:1.4")),
            "# syntax=docker/dockerfile:1.4\nFROM debian\n"
        );
        assert_eq!(apply_syntax_directive(NO_DIRECTIVE, None), NO_DIRECTIVE);
        // `--omit-syntax-directive` drops the user's own directive too.
        assert_eq!(apply_syntax_directive(OLD_FRONTEND, None), "FROM debian\n");
    }
}
