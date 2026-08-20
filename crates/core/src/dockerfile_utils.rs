//! Dockerfile parsing utilities for the compose-features pipeline.
//!
//! This module ports the small subset of the reference DevContainer CLI's
//! `dockerfileUtils.ts` that we need today:
//!
//! - [`ensure_dockerfile_has_final_stage_name`] inspects the final `FROM`
//!   instruction in a user-authored Dockerfile and either returns the existing
//!   stage alias or rewrites the Dockerfile to append a generated alias so the
//!   feature-install layers added later have a deterministic target to
//!   `--target` against.
//! - [`resolve_base_image`] walks the stage graph back from the build's target
//!   stage to the EXTERNAL image it ultimately derives from — the reference's
//!   `findBaseImage`. A caller inspects that image for the
//!   `devcontainer.metadata` it contributes and the `USER` it bakes in, both of
//!   which have to be known BEFORE the build runs because they are written into
//!   the build's own inputs.
//! - [`find_user_statement`] reports the `USER` the target stage ends up running
//!   as when the Dockerfile itself declares one — the reference's
//!   `findUserStatement`. `None` means "ask the base image".
//!
//! The parser is intentionally line-oriented (not a full Dockerfile AST). It
//! mirrors the reference TypeScript regex behavior so we stay in lock-step with
//! upstream parity. Anything more sophisticated (variable substitution, USER
//! resolution, multi-arch arg expansion) is out of scope for bead 14b and
//! belongs in a future expansion of this module.
//!
//! Reference (commit `113500f4`, October 2025):
//! <https://github.com/devcontainers/cli/blob/main/src/spec-node/dockerfileUtils.ts>

use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};
use tracing::{debug, instrument};

/// Matches a complete line that contains a `FROM` instruction, capturing the
/// line text in the `line` group. Multi-line `FROM` continuations (`FROM ... \\\n   alpine`)
/// are not supported by the reference implementation either — both regex
/// engines stop at the first newline character.
static FIND_FROM_LINES: Lazy<Regex> = Lazy::new(|| {
    // (?im) - case-insensitive, multi-line so ^ anchors per line.
    RegexBuilder::new(r"^(?P<line>\s*FROM.*)")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .expect("findFromLines regex must compile")
});

/// Matches a single `FROM` instruction, capturing optional `--platform=...`,
/// the image reference, and an optional `AS <label>` stage alias.
///
/// Mirrors the reference TS regex:
/// `FROM\s+(?<platform>--platform=\S+\s+)?(?<image>"?[^\s]+"?)(\s+AS\s+(?<label>[^\s]+))?`
static PARSE_FROM_LINE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(
        r#"FROM\s+(?P<platform>--platform=\S+\s+)?(?P<image>"?[^\s]+"?)(\s+AS\s+(?P<label>[^\s]+))?"#,
    )
    .case_insensitive(true)
    .build()
    .expect("parseFromLine regex must compile")
});

/// Inspect the final `FROM` instruction in `dockerfile_content` and ensure it
/// has a named stage alias.
///
/// Returns `(modified_dockerfile, final_stage_name)`.
///
/// - If the last `FROM` already declares an `AS <alias>`, the input is
///   returned unchanged and the alias is reported.
/// - Otherwise, the existing `FROM` line is rewritten to append
///   ` AS <default_last_stage_name>` and the modified Dockerfile is returned.
///
/// Handled syntactic shapes (regression-tested below):
/// - `FROM alpine`
/// - `FROM alpine AS build`
/// - `FROM --platform=linux/amd64 alpine`
/// - `FROM --platform=$BUILDPLATFORM scratch`
/// - Multi-stage Dockerfiles where only some stages are aliased
/// - Comments and blank lines between stages
/// - `# syntax=docker/dockerfile:1` parser directives at the file head
/// - Leading whitespace before `FROM`
///
/// # Errors
///
/// Returns `DockerfileParseError::NoFromInstructions` when no `FROM` lines are
/// present, or `DockerfileParseError::MalformedFromLine` when the final `FROM`
/// line cannot be parsed (e.g. `FROM` with no image). These mirror the
/// fail-fast semantics of the reference implementation.
#[instrument(skip(dockerfile_content), fields(default_last_stage_name = %default_last_stage_name))]
pub fn ensure_dockerfile_has_final_stage_name(
    dockerfile_content: &str,
    default_last_stage_name: &str,
) -> Result<(String, String), DockerfileParseError> {
    let from_matches: Vec<_> = FIND_FROM_LINES.captures_iter(dockerfile_content).collect();
    if from_matches.is_empty() {
        return Err(DockerfileParseError::NoFromInstructions);
    }

    let last_from = from_matches.last().expect("non-empty after is_empty check");
    let last_from_full_match = last_from
        .get(0)
        .expect("regex always yields group 0 on a successful match");
    let last_from_line = last_from
        .name("line")
        .expect("findFromLines regex always captures `line`")
        .as_str();

    let from_caps = PARSE_FROM_LINE.captures(last_from_line).ok_or(
        DockerfileParseError::MalformedFromLine {
            line: last_from_line.to_string(),
        },
    )?;

    if let Some(label) = from_caps.name("label") {
        let stage = label.as_str().to_string();
        debug!(stage = %stage, "Final FROM already has stage alias; reusing");
        return Ok((dockerfile_content.to_string(), stage));
    }

    // Compute byte offsets of the matched FROM segment within the whole document
    // so we can splice in ` AS <name>` immediately after it, preserving any
    // trailing whitespace/comment on the same line. This matches the reference
    // implementation's offset arithmetic.
    let line_start_in_doc = last_from_full_match.start();
    let from_caps_match = from_caps
        .get(0)
        .expect("PARSE_FROM_LINE matched, group 0 is always present");
    let from_caps_start_in_line = from_caps_match.start();
    let matched_from_text = from_caps_match.as_str();

    let splice_offset = line_start_in_doc + from_caps_start_in_line + matched_from_text.len();
    let remaining_from_line_len =
        last_from_line.len() - (from_caps_start_in_line + matched_from_text.len());
    let line_end_in_doc = line_start_in_doc + last_from_line.len();
    let resume_offset = line_end_in_doc - remaining_from_line_len;

    let mut modified =
        String::with_capacity(dockerfile_content.len() + default_last_stage_name.len() + 4);
    modified.push_str(&dockerfile_content[..splice_offset]);
    modified.push_str(" AS ");
    modified.push_str(default_last_stage_name);
    modified.push_str(&dockerfile_content[resume_offset..]);

    debug!(
        stage = %default_last_stage_name,
        "Final FROM had no stage alias; appended generated alias"
    );
    Ok((modified, default_last_stage_name.to_string()))
}

/// Matches a global `ARG` declaration (`ARG NAME` or `ARG NAME=default`).
static PARSE_ARG_LINE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^\s*ARG\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)(=(?P<default>.*))?\s*$"#)
        .case_insensitive(true)
        .build()
        .expect("parseArgLine regex must compile")
});

/// Matches a `USER` instruction, capturing the user (and optional `:group`).
static PARSE_USER_LINE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^\s*USER\s+(?P<user>[^\s#]+)"#)
        .case_insensitive(true)
        .build()
        .expect("parseUserLine regex must compile")
});

/// One `FROM` instruction, in document order.
#[derive(Debug, Clone)]
struct Stage {
    /// The image token exactly as written (`alpine:3.19`, `$BASE`, `builder`).
    image: String,
    /// The `AS <alias>` name, when the stage declares one.
    alias: Option<String>,
    /// Byte offset just past this stage's `FROM` line — where its body starts.
    body_start: usize,
    /// Byte offset of this stage's `FROM` line — where the previous body ends.
    from_start: usize,
}

/// Parse every `FROM` instruction in document order.
fn parse_stages(dockerfile_content: &str) -> Vec<Stage> {
    FIND_FROM_LINES
        .captures_iter(dockerfile_content)
        .filter_map(|caps| {
            let full = caps.get(0)?;
            let line = caps.name("line")?.as_str();
            let from = PARSE_FROM_LINE.captures(line)?;
            Some(Stage {
                image: strip_quotes(from.name("image")?.as_str()).to_string(),
                alias: from.name("label").map(|m| m.as_str().to_string()),
                body_start: full.start() + line.len(),
                from_start: full.start(),
            })
        })
        .collect()
}

fn strip_quotes(token: &str) -> &str {
    token.trim_matches('"')
}

/// Collect the ARG declarations that precede the first `FROM` — the only ones
/// BuildKit expands inside a `FROM` — folded with the caller's `--build-arg`
/// overrides, which win.
fn global_args(
    dockerfile_content: &str,
    build_args: &std::collections::HashMap<String, String>,
    first_from: usize,
) -> std::collections::HashMap<String, String> {
    let mut args = std::collections::HashMap::new();
    for line in dockerfile_content[..first_from].lines() {
        if let Some(caps) = PARSE_ARG_LINE.captures(line) {
            let name = caps["name"].to_string();
            let default = caps
                .name("default")
                .map(|m| strip_quotes(m.as_str().trim()).to_string())
                .unwrap_or_default();
            args.insert(name, default);
        }
    }
    for (k, v) in build_args {
        args.insert(k.clone(), v.clone());
    }
    args
}

/// Expand `$NAME` / `${NAME}` occurrences from `args`. Unknown names expand to
/// the empty string, exactly as BuildKit does.
fn expand_args(token: &str, args: &std::collections::HashMap<String, String>) -> String {
    let mut out = String::with_capacity(token.len());
    let bytes: Vec<char> = token.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != '$' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i < bytes.len() && bytes[i] == '{' {
            i += 1;
            let mut name = String::new();
            while i < bytes.len() && bytes[i] != '}' {
                name.push(bytes[i]);
                i += 1;
            }
            i += 1; // consume '}'
            // `${NAME:-default}` / `${NAME:+alt}` are not expanded here; take the
            // name half and let the lookup decide.
            let name = name.split([':', '-']).next().unwrap_or("").to_string();
            out.push_str(args.get(&name).map(String::as_str).unwrap_or(""));
        } else {
            let mut name = String::new();
            while i < bytes.len() && (bytes[i].is_alphanumeric() || bytes[i] == '_') {
                name.push(bytes[i]);
                i += 1;
            }
            out.push_str(args.get(&name).map(String::as_str).unwrap_or(""));
        }
    }
    out
}

/// Index of the stage a build targets: `target` by alias when given, otherwise
/// the last `FROM` in the document.
fn target_stage_index(stages: &[Stage], target: Option<&str>) -> Option<usize> {
    match target {
        Some(t) => stages.iter().rposition(|s| {
            s.alias
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(t))
        }),
        None => stages.len().checked_sub(1),
    }
}

/// Follow one stage's `FROM` to the stage it names, if it names one declared
/// EARLIER in the same document. `None` means the token is an external image.
fn parent_stage_index(stages: &[Stage], idx: usize, image: &str) -> Option<usize> {
    stages[..idx].iter().rposition(|s| {
        s.alias
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(image))
    })
}

/// Resolve the EXTERNAL image the build's target stage ultimately derives from.
///
/// Mirrors the reference CLI's `findBaseImage`: start at `target` (or the final
/// `FROM` when no target is given), follow `FROM <earlier-stage>` links back
/// through the document, and report the first image token that names something
/// outside it. Global `ARG`s — the only ones BuildKit expands in a `FROM` — are
/// substituted, with the caller's `build_args` overriding their defaults.
///
/// Returns `None` when the Dockerfile has no `FROM`, when `target` names no
/// stage, when the chain is cyclic, or when the resolved token still cannot be
/// resolved to a concrete name (an unset `ARG`, `scratch`). A caller treats
/// `None` as "nothing to inherit" rather than as an error: everything this
/// powers is additive metadata.
pub fn resolve_base_image(
    dockerfile_content: &str,
    build_args: &std::collections::HashMap<String, String>,
    target: Option<&str>,
) -> Option<String> {
    let stages = parse_stages(dockerfile_content);
    let mut idx = target_stage_index(&stages, target)?;
    let args = global_args(dockerfile_content, build_args, stages[0].from_start);

    // Bounded by the stage count: each hop moves strictly earlier in the document.
    for _ in 0..=stages.len() {
        let image = expand_args(&stages[idx].image, &args);
        match parent_stage_index(&stages, idx, &image) {
            Some(prev) => idx = prev,
            None => {
                if image.is_empty() || image.eq_ignore_ascii_case("scratch") {
                    return None;
                }
                return Some(image);
            }
        }
    }
    None
}

/// Report the user the build's target stage runs as, when the Dockerfile itself
/// says so.
///
/// Mirrors the reference CLI's `findUserStatement`: scan the target stage's body
/// for its last `USER` instruction and, failing that, follow the stage's `FROM`
/// back through earlier stages in the same document. `None` means no stage in
/// the chain declares one, so the answer belongs to the base image and the
/// caller should inspect it.
pub fn find_user_statement(
    dockerfile_content: &str,
    build_args: &std::collections::HashMap<String, String>,
    target: Option<&str>,
) -> Option<String> {
    let stages = parse_stages(dockerfile_content);
    let mut idx = target_stage_index(&stages, target)?;
    let args = global_args(dockerfile_content, build_args, stages[0].from_start);

    for _ in 0..=stages.len() {
        let body_end = stages
            .get(idx + 1)
            .map(|s| s.from_start)
            .unwrap_or(dockerfile_content.len());
        let body = &dockerfile_content[stages[idx].body_start..body_end];
        // Last USER wins: it is the one in effect when the stage ends.
        let declared = body
            .lines()
            .filter_map(|l| PARSE_USER_LINE.captures(l))
            .map(|c| expand_args(strip_quotes(&c["user"]), &args))
            .rfind(|u| !u.is_empty());
        if declared.is_some() {
            return declared;
        }
        let image = expand_args(&stages[idx].image, &args);
        idx = parent_stage_index(&stages, idx, &image)?;
    }
    None
}

/// Errors returned by the Dockerfile parser.
#[derive(Debug, thiserror::Error)]
pub enum DockerfileParseError {
    /// The Dockerfile contained no `FROM` instructions at all.
    #[error("Dockerfile contains no FROM instructions")]
    NoFromInstructions,
    /// The final `FROM` line could not be parsed (missing image, malformed syntax).
    #[error("failed to parse final FROM line: {line}")]
    MalformedFromLine {
        /// The raw line that failed to parse.
        line: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGE: &str = "dev_containers_target_stage";

    fn ensure(content: &str) -> (String, String) {
        ensure_dockerfile_has_final_stage_name(content, STAGE).expect("parser should succeed")
    }

    #[test]
    fn single_from_without_as_appends_alias() {
        let input = "FROM alpine:3.18\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert_eq!(modified, format!("FROM alpine:3.18 AS {}\n", STAGE));
    }

    #[test]
    fn single_from_with_as_reuses_alias() {
        let input = "FROM alpine:3.18 AS final\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, "final");
        assert_eq!(modified, input, "input should be returned unchanged");
    }

    #[test]
    fn multi_stage_last_stage_has_as_reuses_alias() {
        let input = "FROM alpine AS build\nRUN echo hi\n\nFROM debian:bookworm AS runtime\nCOPY --from=build /app /app\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, "runtime");
        assert_eq!(modified, input);
    }

    #[test]
    fn multi_stage_last_stage_missing_as_appends_alias() {
        let input = "FROM alpine AS build\nRUN echo hi\n\nFROM debian:bookworm\nCOPY --from=build /app /app\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert!(modified.contains(&format!("FROM debian:bookworm AS {}\n", STAGE)));
        // Earlier stages must be left untouched.
        assert!(modified.contains("FROM alpine AS build\n"));
        // Trailing instructions must survive verbatim.
        assert!(modified.contains("COPY --from=build /app /app\n"));
    }

    #[test]
    fn from_with_platform_flag_no_alias_appends() {
        let input = "FROM --platform=linux/amd64 alpine\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert_eq!(
            modified,
            format!("FROM --platform=linux/amd64 alpine AS {}\n", STAGE)
        );
    }

    #[test]
    fn from_with_platform_flag_and_alias_reuses_alias() {
        let input = "FROM --platform=linux/amd64 alpine AS build\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, "build");
        assert_eq!(modified, input);
    }

    #[test]
    fn syntax_directive_preserved_at_top() {
        let input = "# syntax=docker/dockerfile:1\nFROM alpine\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert_eq!(
            modified,
            format!("# syntax=docker/dockerfile:1\nFROM alpine AS {}\n", STAGE)
        );
    }

    #[test]
    fn comments_and_blank_lines_between_stages_are_preserved() {
        let input = "# build stage\nFROM alpine AS build\n\n# this is a comment\nRUN echo hi\n\n# runtime stage\nFROM debian:bookworm\nRUN echo bye\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert!(modified.contains(&format!("FROM debian:bookworm AS {}\n", STAGE)));
        assert!(modified.contains("# build stage\n"));
        assert!(modified.contains("# this is a comment\n"));
        assert!(modified.contains("# runtime stage\n"));
    }

    #[test]
    fn scratch_base_image_handled() {
        let input = "FROM scratch\nCOPY hello /\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert!(modified.starts_with(&format!("FROM scratch AS {}\n", STAGE)));
    }

    #[test]
    fn as_keyword_is_case_insensitive() {
        // Lowercase `as`
        let input_lower = "FROM alpine as final\n";
        let (modified, stage) = ensure(input_lower);
        assert_eq!(stage, "final");
        assert_eq!(modified, input_lower);

        // Mixed-case `As`
        let input_mixed = "FROM alpine As Final\n";
        let (modified, stage) = ensure(input_mixed);
        assert_eq!(stage, "Final");
        assert_eq!(modified, input_mixed);
    }

    #[test]
    fn from_keyword_is_case_insensitive() {
        let input = "from alpine\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert_eq!(modified, format!("from alpine AS {}\n", STAGE));
    }

    #[test]
    fn leading_whitespace_before_from_is_tolerated() {
        let input = "   FROM alpine\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert!(modified.contains(&format!("FROM alpine AS {}\n", STAGE)));
    }

    #[test]
    fn final_from_without_trailing_newline_is_handled() {
        let input = "FROM alpine";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert_eq!(modified, format!("FROM alpine AS {}", STAGE));
    }

    #[test]
    fn from_with_inline_comment_after_image_preserves_comment() {
        // The reference parser only rewrites the matched FROM segment; trailing
        // text on the same line (a comment, in this case) must survive.
        let input = "FROM alpine # base layer\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert_eq!(modified, format!("FROM alpine AS {} # base layer\n", STAGE));
    }

    #[test]
    fn from_with_quoted_image_no_alias_appends() {
        let input = "FROM \"alpine:3.18\"\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert_eq!(modified, format!("FROM \"alpine:3.18\" AS {}\n", STAGE));
    }

    #[test]
    fn dockerfile_with_no_from_returns_error() {
        let input = "# only comments here\nRUN echo hi\n";
        let err = ensure_dockerfile_has_final_stage_name(input, STAGE).unwrap_err();
        matches!(err, DockerfileParseError::NoFromInstructions);
    }

    #[test]
    fn from_with_arg_substitution_is_treated_as_image() {
        // `FROM $BASE` — the regex matches `$BASE` as the image token; no AS,
        // so we should append one.
        let input = "ARG BASE=alpine\nFROM $BASE\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert!(modified.contains(&format!("FROM $BASE AS {}\n", STAGE)));
    }

    fn no_args() -> std::collections::HashMap<String, String> {
        std::collections::HashMap::new()
    }

    #[test]
    fn resolve_base_image_reads_a_single_from() {
        assert_eq!(
            resolve_base_image("FROM alpine:3.19\nRUN true\n", &no_args(), None).as_deref(),
            Some("alpine:3.19")
        );
    }

    #[test]
    fn resolve_base_image_follows_stage_references_to_the_external_image() {
        let df = "FROM debian:bookworm AS base\nRUN true\nFROM base AS mid\nFROM mid\n";
        assert_eq!(
            resolve_base_image(df, &no_args(), None).as_deref(),
            Some("debian:bookworm")
        );
    }

    #[test]
    fn resolve_base_image_honors_the_requested_target() {
        let df = "FROM debian:bookworm AS base\nFROM alpine:3.19 AS other\n";
        assert_eq!(
            resolve_base_image(df, &no_args(), Some("base")).as_deref(),
            Some("debian:bookworm")
        );
        // With no target the LAST FROM wins.
        assert_eq!(
            resolve_base_image(df, &no_args(), None).as_deref(),
            Some("alpine:3.19")
        );
    }

    #[test]
    fn resolve_base_image_expands_global_args_and_build_arg_overrides() {
        let df = "ARG BASE=alpine:3.19\nFROM $BASE\n";
        assert_eq!(
            resolve_base_image(df, &no_args(), None).as_deref(),
            Some("alpine:3.19")
        );

        let mut overrides = std::collections::HashMap::new();
        overrides.insert("BASE".to_string(), "debian:bookworm".to_string());
        assert_eq!(
            resolve_base_image(df, &overrides, None).as_deref(),
            Some("debian:bookworm")
        );

        // `${BRACED}` form, and an unset ARG resolves to nothing rather than to
        // a literal `$NAME` that would 404 against a registry.
        assert_eq!(
            resolve_base_image("ARG B=alpine\nFROM ${B}:3.19\n", &no_args(), None).as_deref(),
            Some("alpine:3.19")
        );
        assert_eq!(resolve_base_image("FROM $UNSET\n", &no_args(), None), None);
    }

    #[test]
    fn resolve_base_image_returns_none_for_unresolvable_bases() {
        assert_eq!(resolve_base_image("RUN true\n", &no_args(), None), None);
        assert_eq!(
            resolve_base_image("FROM scratch\n", &no_args(), None),
            None,
            "`scratch` is not an inspectable image"
        );
        assert_eq!(
            resolve_base_image("FROM alpine AS a\n", &no_args(), Some("nope")),
            None,
            "a target naming no stage resolves to nothing"
        );
    }

    #[test]
    fn find_user_statement_reads_the_last_user_in_the_target_stage() {
        let df = "FROM alpine\nUSER first\nRUN true\nUSER second\n";
        assert_eq!(
            find_user_statement(df, &no_args(), None).as_deref(),
            Some("second")
        );
    }

    #[test]
    fn find_user_statement_follows_the_stage_chain() {
        let df = "FROM alpine AS base\nUSER vscode\nFROM base\nRUN true\n";
        assert_eq!(
            find_user_statement(df, &no_args(), None).as_deref(),
            Some("vscode")
        );
    }

    #[test]
    fn find_user_statement_ignores_users_in_unrelated_stages() {
        // `builder` is not on the final stage's chain, so its USER is not ours.
        let df = "FROM golang AS builder\nUSER nobody\nFROM alpine\nRUN true\n";
        assert_eq!(find_user_statement(df, &no_args(), None), None);
    }

    #[test]
    fn find_user_statement_expands_args() {
        let df = "ARG U=vscode\nFROM alpine\nUSER $U\n";
        assert_eq!(
            find_user_statement(df, &no_args(), None).as_deref(),
            Some("vscode")
        );
    }

    #[test]
    fn multi_stage_with_three_stages_only_modifies_last() {
        let input = "FROM alpine AS s1\nRUN echo 1\nFROM debian AS s2\nRUN echo 2\nFROM ubuntu\nRUN echo 3\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert!(modified.contains("FROM alpine AS s1\n"));
        assert!(modified.contains("FROM debian AS s2\n"));
        assert!(modified.contains(&format!("FROM ubuntu AS {}\n", STAGE)));
    }
}
