//! Boxed text formatting utilities
//!
//! Provides functions to create boxed text sections with Unicode borders
//! for CLI output formatting.

/// Create a boxed text section with a title
///
/// # Arguments
/// * `title` - The title to display in the box header
/// * `content` - The content to display inside the box
///
/// # Returns
/// A formatted string with Unicode box drawing characters
///
/// # Examples
/// ```
/// use deacon_core::text::boxing::boxed_section;
///
/// let content = "This is some content\nwith multiple lines";
/// let boxed = boxed_section("Example", content);
/// println!("{}", boxed);
/// ```
pub fn boxed_section(title: &str, content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let max_line_len = lines.iter().map(|line| line.len()).max().unwrap_or(0);
    let box_width = std::cmp::max(max_line_len, title.len() + 4); // +4 for padding

    let top_border = format!("┌{}┐", "─".repeat(box_width + 2));
    let title_line = format!("│ {:^width$} │", title, width = box_width);
    let title_border = format!("├{}┤", "─".repeat(box_width + 2));
    let bottom_border = format!("└{}┘", "─".repeat(box_width + 2));

    let mut result = vec![top_border, title_line, title_border];

    for line in lines {
        let padded_line = format!("│ {:<width$} │", line, width = box_width);
        result.push(padded_line);
    }

    result.push(bottom_border);
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boxed_section_single_line() {
        let result = boxed_section("Test", "Hello World");
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 5); // top, title, separator, content, bottom
        assert!(result.contains("Test"));
        assert!(result.contains("Hello World"));
    }

    #[test]
    fn test_boxed_section_multi_line() {
        let content = "Line 1\nLine 2\nLine 3";
        let result = boxed_section("Multi", content);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 7); // top, title, separator, 3 content lines, bottom
        assert!(result.contains("Multi"));
        assert!(result.contains("Line 1"));
        assert!(result.contains("Line 2"));
        assert!(result.contains("Line 3"));
    }

    #[test]
    fn test_boxed_section_empty_content() {
        let result = boxed_section("Empty", "");
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 4); // top, title, separator, bottom (no content line)
        assert!(result.contains("Empty"));
    }
}
