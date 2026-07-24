//! Byte-deterministic Markdown prose model (021-normative-clause-inventory,
//! research Decision 1 / data-model.md §2).
//!
//! A tiny, in-crate ATX-heading reader (no Markdown crate — the surface needed is small
//! and must be byte-deterministic across platforms). [`Document::parse`] builds an ordered
//! list of [`Heading`]s with GitHub-style anchors and, for each, the raw text span of its
//! section **including descendants** (the text from the heading to the next heading of the
//! same or a higher level). Parsing is **code-fence aware**: a `#` inside a fenced code
//! block is never mistaken for a heading.
//!
//! The one behavior every command needs is [`Document::contains_excerpt_at`]: the
//! excerpt-present-at-anchor check behind V15 (a clause's verbatim excerpt MUST appear in
//! the pinned document under its recorded heading). Matching is whitespace-normalized so a
//! line-wrapped excerpt still resolves, while remaining a strict substring check.

pub mod normalize;
pub mod strength;

/// One ATX heading and the raw text span of its section (including descendants).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// ATX level (1 for `#`, 2 for `##`, …).
    pub level: usize,
    /// The heading's rendered text (Markdown inline formatting stripped).
    pub text: String,
    /// GitHub-style slug of `text`, deduplicated with a numeric suffix on collision.
    pub anchor: String,
    /// Raw Markdown of this section's body including descendant subsections — from just
    /// after the heading line to the next heading of the same or a higher level.
    pub body: String,
}

/// A parsed Markdown document: its headings (in document order) and any preamble text
/// before the first heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Text before the first heading (often empty for spec docs).
    pub preamble: String,
    /// Headings in document order.
    pub headings: Vec<Heading>,
}

impl Document {
    /// Parse `markdown` into a [`Document`]. Deterministic and code-fence aware.
    pub fn parse(markdown: &str) -> Document {
        let lines: Vec<&str> = markdown.split('\n').collect();

        // First pass: locate heading lines (level, text, line index), skipping fences.
        struct RawHeading {
            level: usize,
            text: String,
            line: usize,
        }
        let mut raw: Vec<RawHeading> = Vec::new();
        let mut in_fence = false;
        let mut fence_marker: &str = "";
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(marker) = fence_open(trimmed) {
                if !in_fence {
                    in_fence = true;
                    fence_marker = marker;
                } else if trimmed.starts_with(fence_marker) {
                    in_fence = false;
                    fence_marker = "";
                }
                continue;
            }
            if in_fence {
                continue;
            }
            if let Some((level, text)) = parse_atx_heading(line) {
                raw.push(RawHeading {
                    level,
                    text,
                    line: idx,
                });
            }
        }

        // Preamble: everything before the first heading line.
        let preamble = if let Some(first) = raw.first() {
            lines[..first.line].join("\n")
        } else {
            markdown.to_string()
        };

        // Second pass: assign each heading its subtree body and a deduplicated anchor.
        let mut headings: Vec<Heading> = Vec::with_capacity(raw.len());
        let mut used_anchors: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (i, h) in raw.iter().enumerate() {
            // Body ends at the next heading of the same or higher level (subtree).
            let body_end = raw
                .iter()
                .skip(i + 1)
                .find(|nxt| nxt.level <= h.level)
                .map(|nxt| nxt.line)
                .unwrap_or(lines.len());
            let body = lines[(h.line + 1)..body_end].join("\n");

            let base = github_slug(&h.text);
            let anchor = match used_anchors.get_mut(&base) {
                Some(count) => {
                    *count += 1;
                    format!("{base}-{count}")
                }
                None => {
                    used_anchors.insert(base.clone(), 0);
                    base
                }
            };
            headings.push(Heading {
                level: h.level,
                text: h.text.clone(),
                anchor,
                body,
            });
        }

        Document { preamble, headings }
    }

    /// The heading with the given anchor, if any.
    pub fn heading(&self, anchor: &str) -> Option<&Heading> {
        self.headings.iter().find(|h| h.anchor == anchor)
    }

    /// All heading anchors in document order.
    pub fn anchors(&self) -> impl Iterator<Item = &str> {
        self.headings.iter().map(|h| h.anchor.as_str())
    }

    /// Whether `excerpt` appears (whitespace-normalized substring) in the section owned by
    /// `anchor` — the V15 excerpt-present-at-anchor check. Returns `false` if the anchor
    /// is unknown or the excerpt is absent from that section's body.
    pub fn contains_excerpt_at(&self, anchor: &str, excerpt: &str) -> bool {
        let Some(heading) = self.heading(anchor) else {
            return false;
        };
        // Search the section body plus the heading text itself (a clause can quote its own
        // heading, e.g. a property name that IS the heading).
        let haystack = collapse_ws(&format!("{}\n{}", heading.text, heading.body));
        let needle = collapse_ws(excerpt);
        !needle.is_empty() && haystack.contains(&needle)
    }
}

/// Collapse every ASCII/Unicode whitespace run in `s` to a single space and trim — the
/// matching basis for [`Document::contains_excerpt_at`] (tolerant of line wrapping, still
/// a strict substring check).
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            prev_space = false;
            out.push(ch);
        }
    }
    out.trim_end().to_string()
}

/// The fenced-code-block opener marker (```` ``` ```` or `~~~`, ≥3) at the head of a
/// (left-trimmed) line, or `None`.
fn fence_open(trimmed: &str) -> Option<&'static str> {
    if trimmed.starts_with("```") {
        Some("```")
    } else if trimmed.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// Parse an ATX heading line (`#`..`######` followed by a space and text), returning
/// `(level, rendered text)`. Trailing `#` runs are stripped (ATX closing sequence).
fn parse_atx_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    // A valid ATX heading requires a space (or end of line) after the `#` run.
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim();
    Some((hashes, strip_inline_markdown(text)))
}

/// Strip inline Markdown from heading text for the rendered form used in slugs/headings:
/// links → label, inline-code backticks and emphasis markers removed.
fn strip_inline_markdown(text: &str) -> String {
    let delinked = normalize::normalize_links_public(text);
    delinked.chars().filter(|&c| c != '`' && c != '*').collect()
}

/// The GitHub-style anchor slug of heading `text`: lowercase; drop every character that is
/// not a letter, digit, space, or hyphen; replace spaces with hyphens. (Duplicate handling
/// is applied by the caller.)
fn github_slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' {
            out.push('-');
        } else if ch == '_' {
            out.push('_');
        }
        // else: dropped (punctuation, backticks, …).
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\
# Title

Intro paragraph.

## Lifecycle scripts

Lead-in text.

### `onCreateCommand`

The `onCreateCommand` command MUST be run only once.

## Another section

```
# not a heading (inside a fence)
```

Text after the fence.
";

    #[test]
    fn parses_headings_with_levels_and_anchors() {
        let doc = Document::parse(DOC);
        let anchors: Vec<&str> = doc.anchors().collect();
        assert_eq!(
            anchors,
            vec![
                "title",
                "lifecycle-scripts",
                "oncreatecommand",
                "another-section"
            ]
        );
        assert_eq!(doc.heading("oncreatecommand").unwrap().level, 3);
    }

    #[test]
    fn code_fence_hash_is_not_a_heading() {
        let doc = Document::parse(DOC);
        assert!(
            doc.anchors().all(|a| a != "not-a-heading"),
            "a # inside a fence must not be parsed as a heading"
        );
    }

    #[test]
    fn subtree_body_includes_descendants() {
        let doc = Document::parse(DOC);
        // `## Lifecycle scripts` owns its `### onCreateCommand` subsection text.
        let body = &doc.heading("lifecycle-scripts").unwrap().body;
        assert!(body.contains("onCreateCommand"), "subtree body: {body}");
    }

    #[test]
    fn contains_excerpt_at_matches_verbatim_and_reflowed() {
        let doc = Document::parse(DOC);
        assert!(doc.contains_excerpt_at(
            "oncreatecommand",
            "The `onCreateCommand` command MUST be run only once."
        ));
        // Whitespace-tolerant.
        assert!(doc.contains_excerpt_at(
            "oncreatecommand",
            "The `onCreateCommand` command\nMUST be run only once."
        ));
        // A sibling section that does not contain the clause → not found (subtree
        // matching means a level-1 ancestor DOES own descendant text, by design).
        assert!(!doc.contains_excerpt_at("another-section", "MUST be run only once."));
        // Fabricated excerpt → not found even under the right anchor.
        assert!(!doc.contains_excerpt_at("oncreatecommand", "MUST be run three times."));
    }

    #[test]
    fn duplicate_headings_get_numeric_suffixes() {
        let md = "# Foo\n\ntext\n\n# Foo\n\nmore\n";
        let doc = Document::parse(md);
        let anchors: Vec<&str> = doc.anchors().collect();
        assert_eq!(anchors, vec!["foo", "foo-1"]);
    }

    #[test]
    fn slug_strips_backticks_and_punctuation() {
        assert_eq!(github_slug("`onCreateCommand`"), "oncreatecommand");
        assert_eq!(github_slug("Feature options (v2)"), "feature-options-v2");
    }
}
