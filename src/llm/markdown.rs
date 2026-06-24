#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownSegment {
    Text(String),
    Code(String),
}

pub fn split_markdown_segments(content: &str) -> Vec<MarkdownSegment> {
    let mut segments = Vec::new();
    let mut in_code = false;
    let mut code_lines = Vec::new();
    let mut text_lines = Vec::new();

    let flush_text = |segments: &mut Vec<MarkdownSegment>, text_lines: &mut Vec<String>| {
        if text_lines.is_empty() {
            return;
        }
        segments.push(MarkdownSegment::Text(text_lines.join("\n")));
        text_lines.clear();
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_code {
                code_lines.push(line.to_string());
                segments.push(MarkdownSegment::Code(code_lines.join("\n")));
                code_lines.clear();
                in_code = false;
            } else {
                flush_text(&mut segments, &mut text_lines);
                in_code = true;
                code_lines.push(line.to_string());
            }
            continue;
        }

        if in_code {
            code_lines.push(line.to_string());
        } else {
            text_lines.push(line.to_string());
        }
    }

    if in_code {
        text_lines.extend(code_lines);
    }
    flush_text(&mut segments, &mut text_lines);
    segments
}

pub fn join_markdown_segments(segments: &[MarkdownSegment]) -> String {
    let mut output = String::new();
    for segment in segments {
        match segment {
            MarkdownSegment::Text(text) if text.trim().is_empty() => continue,
            MarkdownSegment::Text(text) | MarkdownSegment::Code(text) => {
                if !output.is_empty() && !output.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str(text);
            }
        }
    }
    output.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{MarkdownSegment, join_markdown_segments, split_markdown_segments};

    #[test]
    fn split_markdown_segments_preserves_fenced_code_blocks() {
        let content =
            "Intro line\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\nClosing note";
        let segments = split_markdown_segments(content);
        assert_eq!(segments.len(), 3);
        assert!(matches!(&segments[0], MarkdownSegment::Text(text) if text.contains("Intro line")));
        assert!(matches!(&segments[1], MarkdownSegment::Code(code) if code.contains("fn main()")));
        assert!(
            matches!(&segments[2], MarkdownSegment::Text(text) if text.contains("Closing note"))
        );
    }

    #[test]
    fn join_markdown_segments_restores_code_blocks_in_order() {
        let segments = vec![
            MarkdownSegment::Text("Summary".to_string()),
            MarkdownSegment::Code("```bash\necho hi\n```".to_string()),
            MarkdownSegment::Text("Reminder".to_string()),
        ];
        let joined = join_markdown_segments(&segments);
        assert!(joined.contains("Summary"));
        assert!(joined.contains("```bash"));
        assert!(joined.contains("Reminder"));
    }
}
