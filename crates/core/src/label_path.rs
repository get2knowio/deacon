//! Normalization of the paths that go into a container's **identity labels**.
//!
//! `devcontainer.local_folder` and `devcontainer.config_file` are how deacon, the
//! reference CLI and the VS Code Dev Containers extension all recognise "the dev container
//! for this folder", and `${devcontainerId}` is a hash of exactly that pair. So the two
//! implementations must spell one folder identically or neither can see the other's
//! containers, and one folder gets two different ids depending on which tool ran.
//!
//! The reference normalizes these values before writing them and before searching for them
//! ([`normalizeDevContainerLabelPath`], `spec-node/utils.ts:617`, applied in
//! `findContainerAndIdLabels`): the identity off Windows, and on Windows
//! `path.win32.normalize` followed by lowercasing the drive letter. deacon did neither, so
//! on Windows it wrote `C:\Users\me\ws` where the reference writes `c:\Users\me\ws` — a
//! divergence on the *ordinary* spelling, not an exotic one, since `C:\…` is what Explorer
//! and PowerShell hand a user (#682).
//!
//! # Why the whole of `path.win32.normalize` is ported
//!
//! Because the normalizer is not applied only to paths this process produced. A label value
//! read back off a container deacon did not create is a foreign string that never went
//! through [`crate::workspace::absolutize`], and the reference normalizes those too before
//! comparing. A `to_ascii_lowercase` on byte 0 would cover deacon's own output and nothing
//! else.
//!
//! # This tracks node, and node has changed
//!
//! `path.win32.normalize` is not a fixed function: node 20.15/22 rewrote it for
//! CVE-2024-36139, adding the reserved-device-name handling (`CON:`, `COM1:`, `\\?\COM1:`)
//! and the `.\` guard that keeps a relative path from normalizing into something Windows
//! would read as device-absolute. The port below is node 22's, verified against it
//! differentially rather than read off the page; an older node under the reference CLI
//! would answer differently for those inputs, none of which are shapes a workspace path can
//! take.
//!
//! [`normalizeDevContainerLabelPath`]: https://github.com/devcontainers/cli/blob/main/src/spec-node/utils.ts

use std::path::Path;

/// Which platform's rules [`normalize`] should apply.
///
/// Passed explicitly rather than read from `cfg!(windows)` for the same reason the
/// reference's function takes a `NodeJS.Platform` argument: the Windows behavior is then
/// testable from any host, which is how upstream's own
/// `src/test/labelPathNormalization.test.ts` covers it. Production callers want
/// [`Platform::HOST`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Windows — separators, dot segments and the drive letter are all folded.
    Windows,
    /// Everything else — the normalization is the identity.
    Other,
}

impl Platform {
    /// The platform deacon is running on.
    pub const HOST: Self = if cfg!(windows) {
        Self::Windows
    } else {
        Self::Other
    };
}

/// Normalize a path for use as a container identity label value, and for anything derived
/// from one.
///
/// A port of the reference's `normalizeDevContainerLabelPath`. See the module docs for why
/// this exists and how far the port goes.
///
/// ```
/// use deacon_core::label_path::{normalize, Platform};
///
/// // The reference's own three test vectors.
/// assert_eq!(
///     normalize(Platform::Windows, r"C:\CodeBlocks\remill"),
///     r"c:\CodeBlocks\remill"
/// );
/// assert_eq!(
///     normalize(Platform::Windows, "C:/CodeBlocks/remill/x.json"),
///     r"c:\CodeBlocks\remill\x.json"
/// );
/// assert_eq!(
///     normalize(Platform::Other, "/workspaces/remill"),
///     "/workspaces/remill"
/// );
/// ```
pub fn normalize(platform: Platform, value: &str) -> String {
    if platform != Platform::Windows {
        return value.to_string();
    }

    let normalized = win32_normalize(value);
    let bytes = normalized.as_bytes();
    // The reference tests `normalized[1] === ':'`. A `:` at byte 1 already implies byte 0 is
    // a complete one-byte character — no UTF-8 continuation byte is ASCII — so the
    // `is_ascii` guard protects the slice below without changing behavior.
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii() {
        let mut out = String::with_capacity(normalized.len());
        out.push(bytes[0].to_ascii_lowercase() as char);
        out.push_str(&normalized[1..]);
        return out;
    }
    normalized
}

/// The label form of `path`: [`absolutize`](crate::workspace::absolutize) it, then
/// [`normalize`] it for the host platform.
///
/// The one helper every site that stamps or searches for an identity label should call, so
/// "what gets written on the container" and "what `${devcontainerId}` is computed from"
/// cannot drift apart.
pub fn for_path(path: &Path) -> String {
    normalize(
        Platform::HOST,
        &crate::workspace::absolutize(path).to_string_lossy(),
    )
}

/// `\` or `/` — node's `isPathSeparator` on win32.
fn is_path_separator(byte: u8) -> bool {
    byte == b'\\' || byte == b'/'
}

/// An ASCII letter, which is the only thing that can be a drive — node's
/// `isWindowsDeviceRoot`.
fn is_windows_device_root(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

/// The DOS device names Windows still resolves ahead of any file of the same name.
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON",
    "PRN",
    "AUX",
    "NUL",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    "COM\u{b9}",
    "COM\u{b2}",
    "COM\u{b3}",
    "LPT\u{b9}",
    "LPT\u{b2}",
    "LPT\u{b3}",
];

/// node's `isWindowsReservedName(path, colonIndex)`.
///
/// `colonIndex` is a byte index, or `None` where node passes `-1`. That case is not a
/// no-op: node reaches it through `String.prototype.slice(0, -1)`, which drops the LAST
/// character, so `CONx` (no colon at all) tests `CON` and is reserved. Reproduced rather
/// than tidied — it is observable in the output.
fn is_windows_reserved_name(path: &str, colon_index: Option<usize>) -> bool {
    let device_part = match colon_index {
        Some(idx) => &path[..idx],
        None => match path.char_indices().next_back() {
            Some((last, _)) => &path[..last],
            None => path,
        },
    };
    let upper = device_part.to_uppercase();
    WINDOWS_RESERVED_NAMES.contains(&upper.as_str())
}

/// Byte index of the first `:`, node's `indexOf(':')` mapped onto `Option`.
fn colon_index(path: &str) -> Option<usize> {
    path.bytes().position(|b| b == b':')
}

/// A port of node's internal `normalizeString(path, allowAboveRoot, '\\', isPathSeparator)`:
/// collapse `.` and `..` segments and rewrite every separator to `\`.
///
/// Ported statement for statement rather than reimplemented, because the output is compared
/// byte for byte against strings the reference produced. Byte indices stand in for node's
/// UTF-16 ones: the three characters this looks for (`\`, `/`, `.`) are all ASCII, so every
/// index it slices at is a character boundary, and the two length counters it compares are
/// both measured in the same unit.
fn normalize_string_win32(path: &str, allow_above_root: bool) -> String {
    let bytes = path.as_bytes();
    let len = bytes.len();
    let mut res = String::new();
    let mut last_segment_length: i64 = 0;
    let mut last_slash: i64 = -1;
    let mut dots: i64 = 0;
    // Deliberately carried across iterations: at `i == len` node inspects the PREVIOUS byte
    // to decide whether a final segment is still pending.
    let mut code: u8 = 0;

    for i in 0..=len {
        if i < len {
            code = bytes[i];
        } else if is_path_separator(code) {
            break;
        } else {
            code = b'/';
        }

        if is_path_separator(code) {
            if last_slash == i as i64 - 1 || dots == 1 {
                // A repeated separator or a `.` segment contributes nothing.
            } else if dots == 2 {
                let tail_is_parent =
                    res.len() >= 2 && last_segment_length == 2 && res.ends_with("..");
                if !tail_is_parent {
                    if res.len() > 2 {
                        let last_slash_index = res.len() as i64 - last_segment_length - 1;
                        if last_slash_index == -1 {
                            res.clear();
                            last_segment_length = 0;
                        } else {
                            res.truncate(last_slash_index as usize);
                            last_segment_length =
                                res.len() as i64 - 1 - res.rfind('\\').map_or(-1, |j| j as i64);
                        }
                        last_slash = i as i64;
                        dots = 0;
                        continue;
                    } else if !res.is_empty() {
                        res.clear();
                        last_segment_length = 0;
                        last_slash = i as i64;
                        dots = 0;
                        continue;
                    }
                }
                if allow_above_root {
                    if res.is_empty() {
                        res.push_str("..");
                    } else {
                        res.push_str("\\..");
                    }
                    last_segment_length = 2;
                }
            } else {
                let segment = &path[(last_slash + 1) as usize..i];
                if res.is_empty() {
                    res.push_str(segment);
                } else {
                    res.push('\\');
                    res.push_str(segment);
                }
                last_segment_length = i as i64 - last_slash - 1;
            }
            last_slash = i as i64;
            dots = 0;
        } else if code == b'.' && dots != -1 {
            dots += 1;
        } else {
            dots = -1;
        }
    }

    res
}

/// A port of node 22's `path.win32.normalize`, which the reference calls on every identity
/// label value.
///
/// Handles the root shapes Windows has, because each decides differently whether `..` may
/// escape and where the path proper begins: drive-absolute (`C:\x`), drive-relative
/// (`C:x`), rooted (`\x`), UNC (`\\server\share\x`), device (`\\?\…`, `\\.\…`) and the
/// reserved device names.
fn win32_normalize(path: &str) -> String {
    let bytes = path.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return ".".to_string();
    }

    let first = bytes[0];
    if len == 1 {
        // Only a forward slash is rewritten; a lone `\` is already in the target spelling.
        return if first == b'/' {
            "\\".to_string()
        } else {
            path.to_string()
        };
    }

    let mut root_end: usize = 0;
    let mut device: Option<String> = None;
    let mut is_absolute = false;

    if is_path_separator(first) {
        // Leading separator: absolute in some form, UNC or otherwise.
        is_absolute = true;

        if is_path_separator(bytes[1]) {
            let mut j = 2usize;
            let mut last = j;
            while j < len && !is_path_separator(bytes[j]) {
                j += 1;
            }
            if j < len && j != last {
                let first_part = &path[last..j];
                last = j;
                while j < len && is_path_separator(bytes[j]) {
                    j += 1;
                }
                if j < len && j != last {
                    last = j;
                    while j < len && !is_path_separator(bytes[j]) {
                        j += 1;
                    }
                    if j == len || j != last {
                        if first_part == "." || first_part == "?" {
                            // A device root, e.g. `\\.\PHYSICALDRIVE0` or `\\?\C:\…`.
                            device = Some(format!("\\\\{first_part}"));
                            root_end = 4;
                            if let Some(colon) = colon_index(path) {
                                let possible_device = &path[4..colon + 1];
                                let inner =
                                    possible_device.char_indices().next_back().map(|(i, _)| i);
                                if is_windows_reserved_name(possible_device, inner) {
                                    device = Some(format!("\\\\?\\{possible_device}"));
                                    root_end = 4 + possible_device.len();
                                }
                            }
                        } else if j == len {
                            // A UNC root and nothing else; node returns it with a trailing
                            // separator.
                            return format!("\\\\{first_part}\\{}\\", &path[last..]);
                        } else {
                            device = Some(format!("\\\\{first_part}\\{}", &path[last..j]));
                            root_end = j;
                        }
                    }
                }
            }
        } else {
            root_end = 1;
        }
    } else if let Some(colon) = colon_index(path)
        && colon > 0
    {
        if is_windows_device_root(first) && colon == 1 {
            device = Some(path[0..2].to_string());
            root_end = 2;
            if len > 2 && is_path_separator(bytes[2]) {
                is_absolute = true;
                root_end = 3;
            }
        } else if is_windows_reserved_name(path, Some(colon)) {
            device = Some(path[0..colon + 1].to_string());
            root_end = colon + 1;
        }
    }

    let mut tail = if root_end < len {
        normalize_string_win32(&path[root_end..], !is_absolute)
    } else {
        String::new()
    };
    if tail.is_empty() && !is_absolute {
        tail = ".".to_string();
    }
    if !tail.is_empty() && is_path_separator(bytes[len - 1]) {
        tail.push('\\');
    }

    // CVE-2024-36139: a path that was not absolute must not normalize into something
    // Windows would read as device-absolute.
    if !is_absolute && device.is_none() && path.contains(':') {
        let tail_bytes = tail.as_bytes();
        if tail_bytes.len() >= 2 && is_windows_device_root(tail_bytes[0]) && tail_bytes[1] == b':' {
            return format!(".\\{tail}");
        }
        let mut index = colon_index(path);
        while let Some(idx) = index {
            if idx == len - 1 || is_path_separator(bytes[idx + 1]) {
                return format!(".\\{tail}");
            }
            index = path[idx + 1..]
                .bytes()
                .position(|b| b == b':')
                .map(|rel| idx + 1 + rel);
        }
    }

    if is_windows_reserved_name(path, colon_index(path)) {
        return format!(".\\{}{tail}", device.unwrap_or_default());
    }

    match device {
        None => {
            if is_absolute {
                format!("\\{tail}")
            } else {
                tail
            }
        }
        Some(device) => {
            if is_absolute {
                format!("{device}\\{tail}")
            } else {
                format!("{device}{tail}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every expected value below was produced by running `path.win32.normalize(v)`
    /// followed by the reference's drive-letter fold under node, so this table is measured
    /// output rather than a reading of the reference's source (#682).
    ///
    /// The first three rows are upstream's own `src/test/labelPathNormalization.test.ts`
    /// vectors, kept first and unmerged so a later reader can see which part of the table
    /// upstream itself guarantees.
    #[test]
    fn windows_label_paths_match_the_reference_byte_for_byte() {
        let cases: &[(&str, &str)] = &[
            // --- upstream's own three ---
            (r"C:\CodeBlocks\remill", r"c:\CodeBlocks\remill"),
            (
                "C:/CodeBlocks/remill/.devcontainer/devcontainer.json",
                r"c:\CodeBlocks\remill\.devcontainer\devcontainer.json",
            ),
            ("/workspaces/remill", r"\workspaces\remill"),
            // --- the rest of the measured table ---
            (r"c:\foo", r"c:\foo"),
            (r"C:\foo\..\bar", r"c:\bar"),
            (r"C:\foo\.\bar", r"c:\foo\bar"),
            // `..` past the root is dropped, not preserved.
            (r"C:\foo\..\..\bar", r"c:\bar"),
            // A trailing separator survives.
            (r"C:\foo\", r"c:\foo\"),
            (r"C:\", r"c:\"),
            ("C://foo//bar", r"c:\foo\bar"),
            // UNC roots keep both leading separators.
            (r"\\server\share\ws", r"\\server\share\ws"),
            (r"\\server\share\a\..\b", r"\\server\share\b"),
            // Verbatim: byte 1 is `\`, not `:`, so nothing is folded.
            (r"\\?\C:\foo", r"\\?\C:\foo"),
            // Relative forms, which a label should never hold but the function must not
            // mangle if one arrives.
            (r"foo\bar", r"foo\bar"),
            ("", "."),
            ("C:", "c:."),
            ("C:foo", "c:foo"),
            // Only the drive letter folds; the rest keeps its case.
            (r"C:\Foo\BAR", r"c:\Foo\BAR"),
            // CVE-2024-36139: a relative path may not normalize into a device-absolute one.
            ("abC:", r".\abC:"),
            (r"a\..\C:x", r".\C:x"),
            // Reserved device names, which Windows resolves ahead of any real file. Note
            // `CONx`, which has no colon at all: node's `slice(0, -1)` drops the last
            // character and tests `CON`, so the guard fires anyway.
            (r"COM1:\x", r".\COM1:x"),
            ("CONx", r".\CONx"),
            (r"\\.\PHYSICALDRIVE0", r"\\.\PHYSICALDRIVE0"),
            (r"\\?\COM1:\x", r"\\?\COM1:\x"),
        ];

        for (input, expected) in cases {
            assert_eq!(
                normalize(Platform::Windows, input),
                *expected,
                "win32 normalization of {input:?}"
            );
        }
    }

    /// Off Windows the reference returns the value untouched, including values that *look*
    /// like Windows paths — the platform decides, not the shape of the string.
    #[test]
    fn non_windows_label_paths_are_returned_verbatim() {
        for input in [
            "/workspaces/remill",
            "/tmp/a/../b",
            r"C:\CodeBlocks\remill",
            "",
        ] {
            assert_eq!(
                normalize(Platform::Other, input),
                input,
                "non-Windows normalization must be the identity"
            );
        }
    }

    /// Stability under a second pass is what lets a value be normalized on the way in and
    /// again when it is read back off a container without drifting.
    #[test]
    fn windows_label_path_normalization_is_idempotent() {
        for input in [
            r"C:\CodeBlocks\remill",
            "C:/a/../b/",
            r"\\server\share\ws",
            r"\\?\C:\foo",
            "C:",
            r"c:\foo",
        ] {
            let once = normalize(Platform::Windows, input);
            let twice = normalize(Platform::Windows, &once);
            assert_eq!(once, twice, "normalizing {input:?} twice must be stable");
        }
    }

    /// Two spellings that differ only in drive-letter case are the SAME workspace, which is
    /// the property these labels exist to express.
    #[test]
    fn drive_letter_case_does_not_split_one_workspace() {
        assert_eq!(
            normalize(Platform::Windows, r"C:\Users\me\ws"),
            normalize(Platform::Windows, r"c:\Users\me\ws"),
        );
        // ...while a different drive stays a different workspace.
        assert_ne!(
            normalize(Platform::Windows, r"C:\Users\me\ws"),
            normalize(Platform::Windows, r"D:\Users\me\ws"),
        );
    }

    /// `for_path` is `absolutize` composed with the host fold.
    #[test]
    fn for_path_composes_absolutize_with_the_host_fold() {
        let temp = tempfile::TempDir::new().unwrap();
        let expected = normalize(
            Platform::HOST,
            &crate::workspace::absolutize(temp.path()).to_string_lossy(),
        );
        assert_eq!(for_path(temp.path()), expected);
    }

    /// The end-to-end shape of the fix, on the only platform that can run it.
    #[cfg(windows)]
    #[test]
    fn for_path_lowercases_the_drive_on_windows() {
        assert_eq!(for_path(Path::new(r"C:\Users\me\ws")), r"c:\Users\me\ws");
    }
}
