//! The deterministic substance normalizer and fingerprint (021-normative-clause-inventory,
//! research Decision 3).
//!
//! [`normalize_substance`] is a fixed, pure function that reduces a verbatim Markdown
//! excerpt to its normalized substance: lowercase, Markdown formatting stripped
//! (emphasis, inline-code backticks, link syntax, list bullets, block-quote and heading
//! markers), whitespace runs collapsed to a single space, trimmed — while RFC-2119
//! keywords, code-span *contents*, identifiers, and meaningful punctuation (`/`, `.`,
//! `:`) are preserved. [`fingerprint`] is the lowercase-hex SHA-256 of that normalized
//! string.
//!
//! This one small function is the sole source of clause identity, material-vs-immaterial
//! change detection, and immateriality tolerance (SC-005): two excerpts that differ only
//! in whitespace or Markdown reflow normalize to the same string and therefore the same
//! fingerprint.

use sha2::{Digest, Sha256};

/// Reduce a verbatim Markdown `excerpt` to its normalized substance (research
/// Decision 3). Pure and deterministic.
pub fn normalize_substance(excerpt: &str) -> String {
    // 1. Line-oriented pre-pass: strip leading block-quote / list / heading markers so
    //    a bullet or quote wrapper never becomes part of the substance. Whitespace is
    //    collapsed globally afterwards, so per-line trimming here is only about markers.
    let mut cleaned_lines: Vec<String> = Vec::new();
    for raw_line in excerpt.split('\n') {
        let mut line = raw_line.trim_start();
        // Strip any run of block-quote markers ("> ", ">> ", …).
        loop {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix('>') {
                line = rest;
            } else {
                line = t;
                break;
            }
        }
        let trimmed = line.trim_start();
        // Strip a leading ATX heading marker run (`#`, `##`, …).
        let after_heading = trimmed.trim_start_matches('#').trim_start();
        // Strip a leading unordered-list bullet (`- `, `* `, `+ `).
        let after_bullet = strip_list_bullet(after_heading);
        cleaned_lines.push(after_bullet.to_string());
    }
    let joined = cleaned_lines.join("\n");

    // 2. Inline-formatting pass: resolve links to their label, drop inline-code backticks
    //    and emphasis markers, keeping the meaningful content.
    let delinked = strip_links(&joined);

    // 3. Character pass: lowercase; drop backticks and `*`; collapse all whitespace runs
    //    to a single space.
    let mut out = String::with_capacity(delinked.len());
    let mut prev_space = false;
    for ch in delinked.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
            continue;
        }
        prev_space = false;
        match ch {
            '`' | '*' => {} // inline-code / emphasis markers — drop, keep content
            _ => out.extend(ch.to_lowercase()),
        }
    }
    out.trim_end().to_string()
}

/// The lowercase-hex SHA-256 of `normalize_substance(excerpt)` — the clause fingerprint.
pub fn fingerprint(excerpt: &str) -> String {
    let normalized = normalize_substance(excerpt);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Strip a single leading unordered-list bullet (`- `, `* `, `+ `) from a line, leaving
/// ordered-list numerals alone (their digits carry meaning as identifiers).
fn strip_list_bullet(line: &str) -> &str {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest;
        }
    }
    line
}

/// Public wrapper over [`strip_links`] for the heading-text renderer in [`super`].
pub(crate) fn normalize_links_public(text: &str) -> String {
    strip_links(text)
}

/// Replace Markdown link syntax `[label](target)` with just `label` (the visible text),
/// leaving all other text untouched. A malformed/partial link is left verbatim.
fn strip_links(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some((label, consumed)) = parse_link(&text[i..]) {
                out.push_str(&label);
                i += consumed;
                continue;
            }
        }
        // Push one UTF-8 char starting at i.
        let ch = text[i..].chars().next().expect("valid utf-8 boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Parse a `[label](target)` link starting at the head of `s`, returning the label and
/// the number of bytes consumed, or `None` if `s` does not open a complete link.
fn parse_link(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'[') {
        return None;
    }
    let close = s[1..].find(']')? + 1;
    // The `]` must be immediately followed by `(`.
    if bytes.get(close + 1) != Some(&b'(') {
        return None;
    }
    let paren_close_rel = s[close + 2..].find(')')?;
    let paren_close = close + 2 + paren_close_rel;
    let label = s[1..close].to_string();
    Some((label, paren_close + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_and_reflow_are_immaterial() {
        let a = "The   tool  MUST    do X.";
        let b = "The tool\nMUST do X.";
        let c = "The tool MUST do X.";
        assert_eq!(normalize_substance(a), normalize_substance(b));
        assert_eq!(normalize_substance(b), normalize_substance(c));
        assert_eq!(fingerprint(a), fingerprint(c));
    }

    #[test]
    fn markdown_formatting_is_immaterial() {
        let plain = "the onCreateCommand property runs onCreate.";
        let formatted = "The `onCreateCommand` property runs **onCreate**.";
        assert_eq!(
            normalize_substance(plain),
            normalize_substance(formatted),
            "backticks/emphasis/case must not change substance"
        );
    }

    #[test]
    fn links_reduce_to_label() {
        let a = "See [the reference](https://example/ref) for details.";
        let b = "See the reference for details.";
        assert_eq!(normalize_substance(a), normalize_substance(b));
    }

    #[test]
    fn bullets_and_blockquotes_and_headings_are_stripped() {
        assert_eq!(
            normalize_substance("- The tool MUST do X."),
            normalize_substance("The tool MUST do X.")
        );
        assert_eq!(
            normalize_substance("> The tool MUST do X."),
            normalize_substance("The tool MUST do X.")
        );
        assert_eq!(
            normalize_substance("## The tool MUST do X."),
            normalize_substance("The tool MUST do X.")
        );
    }

    #[test]
    fn meaningful_punctuation_and_keywords_preserved() {
        let n = normalize_substance("The path `/workspaces/foo`: it MUST exist.");
        assert!(n.contains('/'), "slashes preserved: {n}");
        assert!(n.contains(':'), "colon preserved: {n}");
        assert!(n.contains("must"), "keyword preserved (lowercased): {n}");
        assert!(!n.contains('`'), "backticks stripped: {n}");
    }

    #[test]
    fn material_difference_changes_fingerprint() {
        assert_ne!(
            fingerprint("The tool MUST do X."),
            fingerprint("The tool MUST do Y.")
        );
        // A strength change is a substance change.
        assert_ne!(
            fingerprint("The tool MUST do X."),
            fingerprint("The tool SHOULD do X.")
        );
    }

    #[test]
    fn fingerprint_is_64_hex_chars_and_deterministic() {
        let f = fingerprint("The tool MUST do X.");
        assert_eq!(f.len(), 64);
        assert!(f.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(f, fingerprint("The   tool MUST do X."));
    }
}
