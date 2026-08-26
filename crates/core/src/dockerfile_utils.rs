//! Dockerfile parsing for the Feature-install pipeline.
//!
//! This is a port of the reference DevContainer CLI's `dockerfileUtils.ts`. The
//! reference reads the user's Dockerfile *before* building it to answer two
//! questions whose answers become inputs to that same build:
//!
//! - [`Dockerfile::base_image`] — the reference's `findBaseImage`. Which EXTERNAL
//!   image does the target stage ultimately derive from? Its
//!   `devcontainer.metadata` is inherited and its baked-in `USER` is what
//!   `_CONTAINER_USER` falls back to.
//! - [`Dockerfile::user_statement`] — the reference's `findUserStatement`. Which
//!   user does the target stage end up running as? That value is exported to every
//!   Feature's `install.sh` as `_REMOTE_USER`/`_CONTAINER_USER`, and is the user
//!   restored after the install layers (`_DEV_CONTAINERS_IMAGE_USER`).
//! - [`ensure_dockerfile_has_final_stage_name`] — the reference's
//!   `ensureDockerfileHasFinalStageName`. Gives the final stage a deterministic
//!   alias so the Feature layers have something to build on.
//!
//! Both of the first two require real variable resolution: `FROM $BASE`,
//! `USER ${USERNAME}`, `ARG`/`ENV` precedence within and across stages, and
//! `${var:+word}` / `${var:-word}` expressions. [`find_value`] and
//! [`replace_variables`] below are statement-for-statement ports of the
//! reference's `findValue` / `replaceVariables` / `getExpressionValue`, because a
//! Dockerfile is an input we do not control and approximating this arithmetic
//! produces confidently wrong answers rather than obviously missing ones — an
//! earlier line-oriented approximation resolved
//! `${cloud:+mcr.microsoft.com/}azure-cli:latest` to `trueazure-cli:latest` (#686).
//!
//! Verified differentially against the reference's own compiled source at the
//! pinned oracle version — see `dockerfile_utils_parity.rs`, whose table is
//! upstream's own test suite plus adversarial cases.
//!
//! Two deliberate departures, both to satisfy the panic-free constitution rather
//! than to change an answer:
//! - A valueless `ENV NAME` resolves to the empty string. The reference asserts
//!   non-null here (`instruction.value!`) and throws a `TypeError` on this input.
//! - `globalBuildxPlatformArgs` (the `TARGETARCH` family) is accepted internally
//!   but no caller supplies it yet; it is the wiring point for multi-arch
//!   `FROM base-${TARGETARCH}` resolution.
//!
//! Reference: <https://github.com/devcontainers/cli/blob/v0.87.0/src/spec-node/dockerfileUtils.ts>

use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};
use std::collections::{HashMap, HashSet};
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

/// The reference's `fromStatement`: anchored per line, used to read the `FROM`
/// that opens a stage.
static FROM_STATEMENT: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(
        r#"^\s*FROM\s+(?P<platform>--platform=\S+\s+)?(?P<image>"?[^\s]+"?)(\s+AS\s+(?P<label>[^\s]+))?"#,
    )
    .case_insensitive(true)
    .multi_line(true)
    .build()
    .expect("fromStatement regex must compile")
});

/// The reference's `argEnvUserStatements`.
static ARG_ENV_USER: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(
        r#"^\s*(?P<instruction>ARG|ENV|USER)\s+(?P<name>[^\s=]+)([ =]+("(?P<value1>\S+)"|(?P<value2>\S+)))?"#,
    )
    .case_insensitive(true)
    .multi_line(true)
    .build()
    .expect("argEnvUserStatements regex must compile")
});

/// The reference's `directives`: `# name=value` at the head of the document.
static DIRECTIVE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*#\s*(?P<name>\S+)\s*=\s*(?P<value>.+)").expect("directives regex must compile")
});

/// The reference's `argumentExpression`: `$NAME`, `${NAME}`, `${NAME:-word}`,
/// `${NAME:+word}`.
static ARGUMENT_EXPRESSION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\$\{?(?P<variable>[a-zA-Z0-9_]+)(?P<isVarExp>:(?P<option>-|\+)(?P<word>[^\}]+))?\}?",
    )
    .expect("argumentExpression regex must compile")
});

/// Positions at which the reference splits the document into stages
/// (`fromStatementsAhead`, a zero-width lookahead the `regex` crate cannot
/// express directly — we take match starts and slice instead).
static FROM_AHEAD: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r"^[\t ]*FROM")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .expect("fromStatementsAhead regex must compile")
});

/// Reads the `syntax=` directive's version, e.g. `docker/dockerfile:1.4`.
static SYNTAX_VERSION: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r"^(?:docker.io/)?docker/dockerfile(?::(?P<version>\S+))?")
        .case_insensitive(true)
        .build()
        .expect("syntax version regex must compile")
});

/// One `ARG` / `ENV` / `USER` instruction, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// The instruction keyword, upper-cased (`ARG`, `ENV`, `USER`).
    pub instruction: String,
    /// The variable name, or for `USER` the user token itself.
    pub name: String,
    /// The value, when the instruction declares one.
    pub value: Option<String>,
}

/// A parsed `FROM` instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct From {
    /// The `--platform=` flag, when present.
    pub platform: Option<String>,
    /// The image token, quotes stripped, variables NOT yet expanded.
    pub image: String,
    /// The `AS <alias>` name, when the stage declares one.
    pub label: Option<String>,
}

/// One build stage: its `FROM` plus the instructions in its body.
#[derive(Debug, Clone)]
pub struct Stage {
    /// The `FROM` that opens this stage.
    pub from: From,
    /// The `ARG`/`ENV`/`USER` instructions in this stage, in document order.
    pub instructions: Vec<Instruction>,
}

/// Everything before the first `FROM`: parser directives and global `ARG`s.
#[derive(Debug, Clone, Default)]
pub struct Preamble {
    /// The `docker/dockerfile` syntax version, when a `syntax=` directive names one.
    pub version: Option<String>,
    /// All `# name=value` parser directives at the head of the document.
    pub directives: HashMap<String, String>,
    /// The global `ARG`s declared before the first `FROM`.
    pub instructions: Vec<Instruction>,
}

/// A parsed Dockerfile — the reference's `Dockerfile` interface.
#[derive(Debug, Clone, Default)]
pub struct Dockerfile {
    /// Everything before the first `FROM`.
    pub preamble: Preamble,
    /// Every stage, in document order.
    pub stages: Vec<Stage>,
    /// Alias → stage index. Built over ALL stages, so a `FROM` may name a stage
    /// declared later in the document, exactly as the reference's
    /// `stagesByLabel` does. Lookup is case-SENSITIVE, matching the reference.
    stages_by_label: HashMap<String, usize>,
}

/// Which instruction list a lookup is currently walking. The reference
/// distinguishes these by object identity; `parent_from` returning `None` for the
/// preamble is what terminates `find_value`'s walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Scope {
    Preamble,
    Stage(usize),
}

/// Strip at most ONE leading and ONE trailing quote, matching the reference's
/// `replace(/^['"]|['"]$/g, '')`. `trim_matches` would strip a run and diverge.
fn strip_one_quote_each_end(token: &str) -> &str {
    let is_quote = |c: char| c == '\'' || c == '"';
    let token = match token.chars().next() {
        Some(c) if is_quote(c) => &token[c.len_utf8()..],
        _ => token,
    };
    match token.chars().next_back() {
        Some(c) if is_quote(c) => &token[..token.len() - c.len_utf8()],
        _ => token,
    }
}

/// The reference's `findLastIndex`: scan backwards from `from`, inclusive.
fn find_last_index<T>(items: &[T], from: i64, pred: impl Fn(&T) -> bool) -> Option<usize> {
    let mut i = from.min(items.len() as i64 - 1);
    while i >= 0 {
        if pred(&items[i as usize]) {
            return Some(i as usize);
        }
        i -= 1;
    }
    None
}

fn parse_from_statement(stage_str: &str) -> From {
    match FROM_STATEMENT.captures(stage_str) {
        None => From {
            platform: None,
            image: "unknown".to_string(),
            label: None,
        },
        Some(caps) => From {
            platform: caps.name("platform").map(|m| m.as_str().to_string()),
            image: strip_one_quote_each_end(
                caps.name("image").map(|m| m.as_str()).unwrap_or_default(),
            )
            .to_string(),
            label: caps.name("label").map(|m| m.as_str().to_string()),
        },
    }
}

fn extract_instructions(stage_str: &str) -> Vec<Instruction> {
    ARG_ENV_USER
        .captures_iter(stage_str)
        .map(|caps| Instruction {
            instruction: caps["instruction"].to_uppercase(),
            name: caps["name"].to_string(),
            value: caps
                .name("value1")
                .or_else(|| caps.name("value2"))
                .map(|m| m.as_str().to_string()),
        })
        .collect()
}

/// Parser directives run from the top of the document and stop at the first line
/// that is not one — the reference `break`s rather than continuing to scan.
fn extract_directives(preamble_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in preamble_str.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match DIRECTIVE.captures(line) {
            Some(caps) => {
                map.entry(caps["name"].to_string())
                    .or_insert_with(|| caps["value"].to_string());
            }
            None => break,
        }
    }
    map
}

/// Parse a Dockerfile — the reference's `extractDockerfile`.
pub fn extract_dockerfile(content: &str) -> Dockerfile {
    let starts: Vec<usize> = FROM_AHEAD.find_iter(content).map(|m| m.start()).collect();

    // The reference splits on a zero-width lookahead, which yields no empty
    // leading element when the document itself starts with a `FROM`.
    let (preamble_str, stage_strs): (&str, Vec<&str>) = if starts.is_empty() {
        (content, Vec::new())
    } else {
        let preamble = if starts[0] == 0 {
            ""
        } else {
            &content[..starts[0]]
        };
        let mut parts = Vec::with_capacity(starts.len());
        for (i, &s) in starts.iter().enumerate() {
            let end = starts.get(i + 1).copied().unwrap_or(content.len());
            parts.push(&content[s..end]);
        }
        (preamble, parts)
    };

    let stages: Vec<Stage> = stage_strs
        .iter()
        .map(|s| Stage {
            from: parse_from_statement(s),
            instructions: extract_instructions(s),
        })
        .collect();

    let directives = extract_directives(preamble_str);
    let version = directives.get("syntax").and_then(|syntax| {
        SYNTAX_VERSION.captures(syntax).map(|caps| {
            caps.name("version")
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "latest".to_string())
        })
    });

    let mut stages_by_label = HashMap::new();
    for (idx, stage) in stages.iter().enumerate() {
        if let Some(label) = &stage.from.label {
            // Later stages overwrite earlier ones, as the reference's reduce does.
            stages_by_label.insert(label.clone(), idx);
        }
    }

    Dockerfile {
        preamble: Preamble {
            version,
            directives,
            instructions: extract_instructions(preamble_str),
        },
        stages,
        stages_by_label,
    }
}

/// The reference's `getExpressionValue`.
fn expression_value(option: &str, is_set: bool, word: &str, value: &str) -> String {
    let picked = match option {
        "-" => {
            if is_set {
                value
            } else {
                word
            }
        }
        "+" => {
            if is_set {
                word
            } else {
                value
            }
        }
        _ => value,
    };
    strip_one_quote_each_end(picked).to_string()
}

impl Dockerfile {
    fn instructions(&self, scope: Scope) -> &[Instruction] {
        match scope {
            Scope::Preamble => &self.preamble.instructions,
            Scope::Stage(i) => &self.stages[i].instructions,
        }
    }

    /// `None` for the preamble — which is what ends `find_value`'s walk.
    fn parent_from(&self, scope: Scope) -> Option<&From> {
        match scope {
            Scope::Preamble => None,
            Scope::Stage(i) => Some(&self.stages[i].from),
        }
    }

    fn stage_named(&self, label: &str) -> Option<Scope> {
        self.stages_by_label.get(label).map(|&i| Scope::Stage(i))
    }

    fn entry_scope(&self, target: Option<&str>) -> Option<Scope> {
        match target {
            Some(t) => self.stage_named(t),
            None => self.stages.len().checked_sub(1).map(Scope::Stage),
        }
    }

    /// The reference's `replaceVariables`: expand every `$VAR` / `${VAR...}` in
    /// `s`, resolving each against `scope` as it stood before instruction
    /// `before`. Rewrites right-to-left so earlier match offsets stay valid.
    fn replace_variables(
        &self,
        build_args: &HashMap<String, String>,
        base_image_env: &HashMap<String, String>,
        global_buildx_args: &HashMap<String, String>,
        s: &str,
        scope: Scope,
        before: i64,
    ) -> String {
        let mut result = s.to_string();
        for caps in ARGUMENT_EXPRESSION
            .captures_iter(s)
            .collect::<Vec<_>>()
            .iter()
            .rev()
        {
            let whole = match caps.get(0) {
                Some(m) => m,
                None => continue,
            };
            let variable = match caps.name("variable") {
                Some(m) => m.as_str(),
                None => continue,
            };
            let mut value = self
                .find_value(
                    build_args,
                    base_image_env,
                    global_buildx_args,
                    variable,
                    scope,
                    before,
                )
                .unwrap_or_default();
            if caps.name("isVarExp").is_some() {
                let option = caps.name("option").map(|m| m.as_str()).unwrap_or("");
                let word = caps.name("word").map(|m| m.as_str()).unwrap_or("");
                let is_set = !value.is_empty();
                value = expression_value(option, is_set, word, &value);
            }
            result.replace_range(whole.start()..whole.end(), &value);
        }
        result
    }

    /// The reference's `findValue`: walk backwards through the current scope's
    /// instructions for the last `ENV`, or `ARG` that actually has a value, then
    /// follow the stage's `FROM` and continue. `ARG`s only count in the scope the
    /// lookup started in and in the preamble — an inherited stage contributes its
    /// `ENV`s but not its `ARG`s, which is why `ARG` after `ENV` in a *preceding*
    /// stage still resolves to the `ENV`.
    #[allow(clippy::too_many_arguments)]
    fn find_value(
        &self,
        build_args: &HashMap<String, String>,
        base_image_env: &HashMap<String, String>,
        global_buildx_args: &HashMap<String, String>,
        variable: &str,
        scope: Scope,
        before: i64,
    ) -> Option<String> {
        let mut scope = scope;
        let mut before = before;
        let mut consider_arg = true;
        let mut seen: HashSet<Scope> = HashSet::new();

        loop {
            if !seen.insert(scope) {
                return None;
            }

            let instructions = self.instructions(scope);
            let found = find_last_index(instructions, before - 1, |i| {
                i.name == variable
                    && (i.instruction == "ENV"
                        || (consider_arg
                            && (build_args.contains_key(&i.name) || i.value.is_some())))
            });

            if let Some(idx) = found {
                let instruction = &instructions[idx];
                if instruction.instruction == "ENV" {
                    // A valueless `ENV NAME` resolves to empty rather than
                    // throwing, per the module note.
                    let value = instruction.value.clone().unwrap_or_default();
                    return Some(self.replace_variables(
                        build_args,
                        base_image_env,
                        global_buildx_args,
                        &value,
                        scope,
                        idx as i64,
                    ));
                }
                if instruction.instruction == "ARG" {
                    let value = build_args
                        .get(&instruction.name)
                        .cloned()
                        .or_else(|| instruction.value.clone())
                        .unwrap_or_default();
                    return Some(self.replace_variables(
                        build_args,
                        base_image_env,
                        global_buildx_args,
                        &value,
                        scope,
                        idx as i64,
                    ));
                }
            }

            let from = match self.parent_from(scope) {
                // The preamble is the end of the chain: fall back to the base
                // image's environment, then to the buildx platform args.
                None => {
                    return base_image_env
                        .get(variable)
                        .or_else(|| global_buildx_args.get(variable))
                        .cloned();
                }
                Some(from) => from,
            };

            let image = self.replace_variables(
                build_args,
                base_image_env,
                global_buildx_args,
                &from.image,
                Scope::Preamble,
                self.preamble.instructions.len() as i64,
            );
            scope = self.stage_named(&image).unwrap_or(Scope::Preamble);
            before = self.instructions(scope).len() as i64;
            consider_arg = scope == Scope::Preamble;
        }
    }

    /// The reference's `findBaseImage`: the EXTERNAL image the target stage
    /// ultimately derives from.
    ///
    /// Returns exactly what the reference returns, including `"scratch"` and the
    /// empty string for an unresolvable `ARG`. Callers that need an *inspectable*
    /// image should use [`resolve_base_image`], which applies that guard.
    ///
    /// `None` when the document has no stages, when `target` names no stage, or
    /// when the stage chain is cyclic.
    pub fn base_image(
        &self,
        build_args: &HashMap<String, String>,
        target: Option<&str>,
    ) -> Option<String> {
        // The reference passes an empty baseImageEnv here: ENV is not available
        // to a FROM instruction.
        let empty = HashMap::new();
        let mut scope = self.entry_scope(target)?;
        let mut seen: HashSet<Scope> = HashSet::new();

        loop {
            if !seen.insert(scope) {
                return None;
            }
            let from = self.parent_from(scope)?;
            let image = self.replace_variables(
                build_args,
                &empty,
                &empty,
                &from.image,
                Scope::Preamble,
                self.preamble.instructions.len() as i64,
            );
            match self.stage_named(&image) {
                None => return Some(image),
                Some(next) => scope = next,
            }
        }
    }

    /// The reference's `findUserStatement`: the user the target stage ends up
    /// running as, or `None` when no stage in the chain declares one and the
    /// answer belongs to the base image.
    pub fn user_statement(
        &self,
        build_args: &HashMap<String, String>,
        base_image_env: &HashMap<String, String>,
        target: Option<&str>,
    ) -> Option<String> {
        let empty = HashMap::new();
        let mut scope = self.entry_scope(target)?;
        let mut seen: HashSet<Scope> = HashSet::new();

        loop {
            if !seen.insert(scope) {
                return None;
            }
            let instructions = self.instructions(scope);
            if let Some(idx) = find_last_index(instructions, instructions.len() as i64 - 1, |i| {
                i.instruction == "USER"
            }) {
                let resolved = self.replace_variables(
                    build_args,
                    base_image_env,
                    &empty,
                    &instructions[idx].name,
                    scope,
                    idx as i64,
                );
                // The reference's `|| undefined`: an empty expansion is no answer.
                return if resolved.is_empty() {
                    None
                } else {
                    Some(resolved)
                };
            }
            let from = self.parent_from(scope)?;
            let image = self.replace_variables(
                build_args,
                base_image_env,
                &empty,
                &from.image,
                Scope::Preamble,
                self.preamble.instructions.len() as i64,
            );
            scope = self.stage_named(&image)?;
        }
    }
}

/// Resolve the EXTERNAL image the build's target stage derives from, restricted
/// to images that can actually be inspected.
///
/// This is [`Dockerfile::base_image`] plus the guard our callers need: the
/// reference reports `scratch` and the empty string (an `ARG` that resolved to
/// nothing) as base images, but neither can be pulled or inspected, and every
/// consumer here uses the result to read `devcontainer.metadata` and the baked-in
/// `USER` off a real image. `None` means "nothing to inherit", not an error.
pub fn resolve_base_image(
    dockerfile_content: &str,
    build_args: &HashMap<String, String>,
    target: Option<&str>,
) -> Option<String> {
    extract_dockerfile(dockerfile_content)
        .base_image(build_args, target)
        .filter(|image| !image.is_empty() && !image.eq_ignore_ascii_case("scratch"))
}

/// Report the user the build's target stage runs as, when the Dockerfile itself
/// says so.
///
/// `base_image_env` is the environment of the image the Dockerfile derives from;
/// a `USER ${NAME}` whose `NAME` is set only in the base image resolves through
/// it. Pass an empty map when the base image has not been inspected.
pub fn find_user_statement(
    dockerfile_content: &str,
    build_args: &HashMap<String, String>,
    base_image_env: &HashMap<String, String>,
    target: Option<&str>,
) -> Option<String> {
    extract_dockerfile(dockerfile_content).user_statement(build_args, base_image_env, target)
}

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

    #[test]
    fn multi_stage_with_three_stages_only_modifies_last() {
        let input = "FROM alpine AS s1\nRUN echo 1\nFROM debian AS s2\nRUN echo 2\nFROM ubuntu\nRUN echo 3\n";
        let (modified, stage) = ensure(input);
        assert_eq!(stage, STAGE);
        assert!(modified.contains("FROM alpine AS s1\n"));
        assert!(modified.contains("FROM debian AS s2\n"));
        assert!(modified.contains(&format!("FROM ubuntu AS {}\n", STAGE)));
    }

    fn no_args() -> HashMap<String, String> {
        HashMap::new()
    }

    fn args(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn base_image_of(
        content: &str,
        build_args: &HashMap<String, String>,
        target: Option<&str>,
    ) -> Option<String> {
        extract_dockerfile(content).base_image(build_args, target)
    }

    fn user_of(
        content: &str,
        build_args: &HashMap<String, String>,
        target: Option<&str>,
    ) -> Option<String> {
        extract_dockerfile(content).user_statement(build_args, &no_args(), target)
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

        assert_eq!(
            resolve_base_image(df, &args(&[("BASE", "debian:bookworm")]), None).as_deref(),
            Some("debian:bookworm")
        );

        // `${BRACED}` form, and an unset ARG resolves to nothing rather than
        // to a literal `$NAME` that would 404 against a registry.
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

    /// The guard `resolve_base_image` applies is OURS, not the reference's: the
    /// reference reports these verbatim and its callers cope. Keeping both
    /// visible is what stops the guard from being mistaken for parity.
    #[test]
    fn base_image_reports_what_the_reference_reports_before_our_guard() {
        assert_eq!(
            base_image_of("FROM scratch\n", &no_args(), None).as_deref(),
            Some("scratch")
        );
        assert_eq!(
            base_image_of("FROM $UNSET\n", &no_args(), None).as_deref(),
            Some("")
        );
        assert_eq!(resolve_base_image("FROM scratch\n", &no_args(), None), None);
        assert_eq!(resolve_base_image("FROM $UNSET\n", &no_args(), None), None);
    }

    #[test]
    fn find_user_statement_reads_the_last_user_in_the_target_stage() {
        let df = "FROM alpine\nUSER first\nRUN true\nUSER second\n";
        assert_eq!(user_of(df, &no_args(), None).as_deref(), Some("second"));
    }

    #[test]
    fn find_user_statement_follows_the_stage_chain() {
        let df = "FROM alpine AS base\nUSER vscode\nFROM base\nRUN true\n";
        assert_eq!(user_of(df, &no_args(), None).as_deref(), Some("vscode"));
    }

    #[test]
    fn find_user_statement_ignores_users_in_unrelated_stages() {
        // `builder` is not on the final stage's chain, so its USER is not ours.
        let df = "FROM golang AS builder\nUSER nobody\nFROM alpine\nRUN true\n";
        assert_eq!(user_of(df, &no_args(), None), None);
    }

    #[test]
    fn find_user_statement_expands_args() {
        let df = "ARG U=vscode\nFROM alpine\nUSER $U\n";
        assert_eq!(user_of(df, &no_args(), None).as_deref(), Some("vscode"));
    }

    /// #686: the defect that made `deacon build` hand every Feature
    /// `_REMOTE_USER=root` on a Dockerfile whose USER came from an in-stage ARG.
    #[test]
    fn find_user_statement_resolves_in_stage_args_and_envs() {
        assert_eq!(
            user_of(
                "FROM debian\nARG IMAGE_USER=user2\nUSER $IMAGE_USER\n",
                &no_args(),
                None
            )
            .as_deref(),
            Some("user2")
        );
        assert_eq!(
            user_of(
                "FROM debian\nARG IMAGE_USER=user2\nUSER $IMAGE_USER\n",
                &args(&[("IMAGE_USER", "user3")]),
                None
            )
            .as_deref(),
            Some("user3")
        );
        // ENV wins over an ARG declared after it; an unbound ARG does not shadow.
        assert_eq!(
            user_of(
                "\nFROM debian\nENV USERNAME=user1\nARG USERNAME=user2\nUSER ${USERNAME}\n",
                &no_args(),
                None
            )
            .as_deref(),
            Some("user2")
        );
        assert_eq!(
            user_of(
                "\nFROM debian\nENV USERNAME=user1\nARG USERNAME\nUSER ${USERNAME}\n",
                &no_args(),
                None
            )
            .as_deref(),
            Some("user1")
        );
        // An ENV can be set from an ARG, and several variables can share a token.
        assert_eq!(
            user_of(
                "\nFROM debian\nARG USERNAME1=user1\nENV USERNAME2=${USERNAME1}\nUSER ${USERNAME2}\n",
                &no_args(),
                None
            )
            .as_deref(),
            Some("user1")
        );
        assert_eq!(
            user_of(
                "\nFROM debian\nARG USERNAME1=user1\nENV USERNAME2=user2\nUSER A${USERNAME1}A${USERNAME2}A\n",
                &no_args(),
                None
            )
            .as_deref(),
            Some("Auser1Auser2A")
        );
    }

    /// An inherited stage contributes its ENVs but NOT its ARGs — which is why
    /// this resolves to `user1` and not `user2`.
    #[test]
    fn args_do_not_cross_a_stage_boundary_but_envs_do() {
        let df = "\nFROM debian as one\nENV USERNAME=user1\nARG USERNAME=user2\n\nFROM one as two\nUSER ${USERNAME}\n";
        assert_eq!(user_of(df, &no_args(), None).as_deref(), Some("user1"));
    }

    #[test]
    fn find_user_statement_falls_back_to_the_base_image_env() {
        let df = "\nFROM mybase\nUSER ${USERNAME}\n";
        assert_eq!(
            extract_dockerfile(df)
                .user_statement(&no_args(), &args(&[("USERNAME", "user1")]), None)
                .as_deref(),
            Some("user1")
        );
        // Without the base image's env there is no answer at all.
        assert_eq!(user_of(df, &no_args(), None), None);
    }

    /// A `FROM` may name a stage declared LATER in the document. Resolving only
    /// backwards returned the stage name itself, which we would then have tried
    /// to pull as if it were an image (#686).
    #[test]
    fn stage_references_resolve_in_both_directions() {
        let df = "\nFROM image1 as stage1\nFROM stage3 as stage2\nFROM image3 as stage3\nFROM image4 as stage4\n";
        assert_eq!(
            base_image_of(df, &no_args(), Some("stage2")).as_deref(),
            Some("image3")
        );
    }

    #[test]
    fn variable_expressions_choose_between_word_and_value() {
        let pos = "\nARG cloud\nFROM ${cloud:+mcr.microsoft.com/}azure-cli:latest\n";
        assert_eq!(
            base_image_of(pos, &args(&[("cloud", "true")]), None).as_deref(),
            Some("mcr.microsoft.com/azure-cli:latest")
        );
        assert_eq!(
            base_image_of(pos, &no_args(), None).as_deref(),
            Some("azure-cli:latest")
        );

        let neg = "\nARG cloud\nFROM ${cloud:-mcr.microsoft.com/}azure-cli:latest\n";
        assert_eq!(
            base_image_of(neg, &args(&[("cloud", "ghcr.io/")]), None).as_deref(),
            Some("ghcr.io/azure-cli:latest")
        );
        assert_eq!(
            base_image_of(neg, &no_args(), None).as_deref(),
            Some("mcr.microsoft.com/azure-cli:latest")
        );

        // The chosen word has one layer of quotes stripped.
        let quoted =
            "\nARG cloud\nFROM ${cloud:-\"mcr.microsoft.com/\"}azure-cli:latest as label\n";
        assert_eq!(
            base_image_of(quoted, &no_args(), None).as_deref(),
            Some("mcr.microsoft.com/azure-cli:latest")
        );
    }

    #[test]
    fn cyclic_stage_chains_terminate_with_no_answer() {
        assert_eq!(
            base_image_of("FROM b as a\nFROM a as b\n", &no_args(), None),
            None
        );
        assert_eq!(base_image_of("FROM a as a\n", &no_args(), None), None);
        // A cycle is not the same as no answer: the walk stops at the first
        // stage that declares a USER, and only runs out of stages when none
        // does. MEASURED — the reference returns "x" here too, and the
        // hand-written `None` this once asserted was simply wrong.
        assert_eq!(
            user_of("FROM b as a\nUSER x\nFROM a as b\n", &no_args(), None).as_deref(),
            Some("x")
        );
    }

    /// Stage labels are matched case-SENSITIVELY, as the reference's plain-object
    /// `stagesByLabel` does. BuildKit itself is case-insensitive here, so this is
    /// a deliberate follow-the-reference choice, not an oversight: the value only
    /// feeds metadata inspection, and the real build is resolved by BuildKit.
    #[test]
    fn stage_labels_match_case_sensitively_like_the_reference() {
        assert_eq!(
            base_image_of("FROM alpine AS Base\n", &no_args(), Some("base")),
            None
        );
        assert_eq!(
            base_image_of("FROM alpine AS Base\nFROM base\n", &no_args(), None).as_deref(),
            Some("base"),
            "`base` names no stage, so it is reported as an external image"
        );
    }

    #[test]
    fn quotes_are_stripped_one_layer_at_each_end() {
        assert_eq!(strip_one_quote_each_end("\"abc\""), "abc");
        assert_eq!(strip_one_quote_each_end("\"\"abc\"\""), "\"abc\"");
        assert_eq!(strip_one_quote_each_end("\""), "");
        assert_eq!(strip_one_quote_each_end("\"\""), "");
        assert_eq!(strip_one_quote_each_end("abc"), "abc");
    }

    #[test]
    fn extract_dockerfile_reads_instructions_and_directives() {
        let df = extract_dockerfile("from E\nenv A=B\narg C\nuser D\n");
        assert_eq!(df.stages.len(), 1);
        assert_eq!(df.stages[0].from.image, "E");
        let instrs = &df.stages[0].instructions;
        assert_eq!(instrs.len(), 3);
        assert_eq!(instrs[0].instruction, "ENV");
        assert_eq!(instrs[0].name, "A");
        assert_eq!(instrs[0].value.as_deref(), Some("B"));
        assert_eq!(instrs[1].instruction, "ARG");
        assert_eq!(instrs[1].name, "C");
        assert_eq!(instrs[1].value, None);
        assert_eq!(instrs[2].instruction, "USER");
        assert_eq!(instrs[2].name, "D");

        // `ENV A B` and `ENV A = B` are the same instruction as `ENV A=B`.
        for src in [
            "FROM debian\nENV A=B",
            "FROM debian\nENV A = B",
            "FROM debian\nENV A B",
        ] {
            let env = &extract_dockerfile(src).stages[0].instructions[0];
            assert_eq!(
                (
                    env.instruction.as_str(),
                    env.name.as_str(),
                    env.value.as_deref()
                ),
                ("ENV", "A", Some("B")),
                "for {src:?}"
            );
        }

        let syntax = extract_dockerfile("# syntax=docker.io/docker/dockerfile:1.4\nFROM debian");
        assert_eq!(syntax.preamble.version.as_deref(), Some("1.4"));
        let untagged = extract_dockerfile("# syntax=docker/dockerfile\nFROM debian");
        assert_eq!(untagged.preamble.version.as_deref(), Some("latest"));
        let foreign = extract_dockerfile("# syntax=mycompany/myimage:1.4\nFROM debian");
        assert_eq!(foreign.preamble.version, None);
        assert!(foreign.preamble.directives.contains_key("syntax"));
    }
}
