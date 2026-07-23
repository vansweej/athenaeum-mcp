use crate::error::IngestError;
use epub::doc::EpubDoc;
use pdfium_render::prelude::*;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExtractedDocument {
    pub title: String,
    pub pages: Vec<ExtractedPage>,
}

#[derive(Debug, Clone)]
pub struct ExtractedPage {
    pub page_number: u32,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct EpubSection {
    pub chapter: Option<String>,
    pub section: Option<String>,
    pub text: String,
}

#[cfg(not(tarpaulin_include))]
pub async fn extract_pdf(path: &Path) -> Result<ExtractedDocument, IngestError> {
    let pdfium = Pdfium::default();
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| IngestError::ParseFailed(format!("failed to load pdf: {}", e)))?;

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut pages = Vec::new();

    for (i, page) in document.pages().iter().enumerate() {
        let text = page
            .text()
            .map_err(|e| IngestError::ParseFailed(format!("failed to extract text: {}", e)))?
            .all();

        if !text.trim().is_empty() {
            pages.push(ExtractedPage {
                page_number: (i + 1) as u32,
                text,
            });
        }
    }

    if pages.is_empty() {
        return Err(IngestError::ParseFailed("empty document".to_string()));
    }

    Ok(ExtractedDocument { title, pages })
}

pub async fn extract_epub(path: &Path) -> Result<(String, Vec<EpubSection>), IngestError> {
    let mut doc = EpubDoc::new(path)
        .map_err(|e| IngestError::ParseFailed(format!("failed to open epub: {}", e)))?;

    let title = doc
        .mdata("title")
        .map(|m| m.value.clone())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    let mut sections = Vec::new();
    let mut current_chapter: Option<String> = None;
    let mut current_section: Option<String> = None;

    // Iterate through spine items
    while let Some((content, _mime)) = doc.get_current_str() {
        // Extract chapter from <h1> tags
        if let Some(h1_text) = extract_heading(&content, "h1") {
            current_chapter = Some(h1_text);
            current_section = None; // Reset section when chapter changes
        }

        // Extract section from <h2> tags
        if let Some(h2_text) = extract_heading(&content, "h2") {
            current_section = Some(h2_text);
        }

        // Strip HTML tags to get plain text
        let text = strip_html_tags(&content);

        if !text.trim().is_empty() {
            sections.push(EpubSection {
                chapter: current_chapter.clone(),
                section: current_section.clone(),
                text,
            });
        }

        // Move to next spine item
        if !doc.go_next() {
            break;
        }
    }

    if sections.is_empty() {
        return Err(IngestError::ParseFailed("empty document".to_string()));
    }

    Ok((title, sections))
}

pub async fn extract_md(path: &Path) -> Result<(String, Vec<EpubSection>), IngestError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| IngestError::IoFailed(e.to_string()))?;

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let content = strip_front_matter(&raw);
    let sections = parse_markdown_sections(content);

    if sections.is_empty() {
        return Err(IngestError::ParseFailed("empty document".to_string()));
    }

    Ok((title, sections))
}

/// Strip a leading YAML front-matter block (`---` … `---`) from the content.
///
/// If the content does not begin with exactly `---` (followed by a newline) or
/// there is no closing `---` fence, the original slice is returned unchanged.
fn strip_front_matter(content: &str) -> &str {
    // Opening fence must be the very first line.
    let rest = if let Some(r) = content.strip_prefix("---\r\n") {
        r
    } else if let Some(r) = content.strip_prefix("---\n") {
        r
    } else {
        return content;
    };

    // Find the closing fence.
    for (offset, line) in line_offsets(rest) {
        if line == "---" {
            // Return everything after the closing fence line (including its newline).
            let after = offset + line.len();
            let after = after + rest[after..].find('\n').map_or(0, |i| i + 1);
            return &rest[after..];
        }
    }

    // No closing fence — return original unchanged.
    content
}

/// Iterate `(byte_offset_of_line_start, line_without_newline)` over `s`.
fn line_offsets(s: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    s.split('\n').map(move |line| {
        let start = offset;
        // +1 for the '\n' that split consumed (may overshoot at EOF, harmless).
        offset += line.len() + 1;
        // Strip the optional '\r' so lines ending with \r\n compare cleanly.
        (start, line.trim_end_matches('\r'))
    })
}

/// Parse Markdown content into [`EpubSection`]s using pulldown-cmark.
///
/// Rules:
/// - `# H1` → flush current body, set `current_chapter`, reset `current_section`.
/// - `## H2` → flush current body, set `current_section`.
/// - `### H3+` → heading text folded into the body of the current section.
/// - Body text accumulates via `Text` and `Code` events; `SoftBreak`/`HardBreak`
///   become spaces.
/// - Each heading *Start* triggers a flush of the body accumulated under the
///   previous heading (flush-before-apply ordering).
fn parse_markdown_sections(content: &str) -> Vec<EpubSection> {
    let mut sections: Vec<EpubSection> = Vec::new();
    let mut current_chapter: Option<String> = None;
    let mut current_section: Option<String> = None;
    let mut body: Vec<String> = Vec::new();
    let mut in_heading = false;
    let mut heading_buf = String::new();
    let mut pending_level: Option<HeadingLevel> = None;

    let parser = Parser::new(content);

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                // Flush whatever body accumulated before this heading.
                flush_section(&mut sections, &current_chapter, &current_section, &mut body);
                in_heading = true;
                heading_buf.clear();
                pending_level = Some(level);
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                let h = heading_buf.trim().to_string();
                match pending_level {
                    Some(HeadingLevel::H1) => {
                        current_chapter = Some(h);
                        current_section = None;
                    }
                    Some(HeadingLevel::H2) => {
                        current_section = Some(h);
                    }
                    _ => {
                        // H3+ folded into body.
                        body.push(h);
                    }
                }
                pending_level = None;
            }
            Event::Text(t) | Event::Code(t) => {
                if in_heading {
                    heading_buf.push_str(&t);
                } else {
                    body.push(t.into_string());
                }
            }
            Event::SoftBreak | Event::HardBreak if !in_heading => {
                body.push(" ".to_string());
            }
            _ => {}
        }
    }

    // Flush any trailing body.
    flush_section(&mut sections, &current_chapter, &current_section, &mut body);

    sections
}

/// Drain `body`, normalise whitespace, and push a non-empty [`EpubSection`].
fn flush_section(
    sections: &mut Vec<EpubSection>,
    chapter: &Option<String>,
    section: &Option<String>,
    body: &mut Vec<String>,
) {
    let raw: String = body.drain(..).collect();
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.is_empty() {
        sections.push(EpubSection {
            chapter: chapter.clone(),
            section: section.clone(),
            text,
        });
    }
}

/// Extract the first heading of a given tag type from HTML content
fn extract_heading(html: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    if let Some(start) = html.find(&open_tag) {
        let start_pos = start + open_tag.len();
        if let Some(end) = html[start_pos..].find(&close_tag) {
            let heading_text = &html[start_pos..start_pos + end];
            return Some(heading_text.trim().to_string());
        }
    }
    None
}

/// Strip all HTML tags from content, leaving only plain text
fn strip_html_tags(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    // Clean up whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_front_matter_no_front_matter() {
        let s = "# Title\n\nSome content.\n";
        assert_eq!(strip_front_matter(s), s);
    }

    #[test]
    fn strip_front_matter_well_formed() {
        let s = "---\ntitle: Foo\n---\n# Title\n\nContent.\n";
        assert_eq!(strip_front_matter(s), "# Title\n\nContent.\n");
    }

    #[test]
    fn strip_front_matter_well_formed_crlf() {
        let s = "---\r\ntitle: Foo\r\n---\r\n# Title\r\n";
        // After the closing ---, the next char is \r\n → consume \n
        let result = strip_front_matter(s);
        assert!(result.starts_with("# Title"));
    }

    #[test]
    fn strip_front_matter_unterminated() {
        let s = "---\ntitle: Foo\n# Title\n";
        // No closing fence → return original unchanged.
        assert_eq!(strip_front_matter(s), s);
    }

    #[test]
    fn parse_markdown_h1_sets_chapter() {
        let sections = parse_markdown_sections("# Chapter One\n\nSome prose here.\n");
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].chapter.as_deref(), Some("Chapter One"));
        assert_eq!(sections[0].section, None);
        assert!(sections[0].text.contains("Some prose here"));
    }

    #[test]
    fn parse_markdown_h2_sets_section() {
        let md = "# Chapter One\n\n## Section A\n\nProse under section.\n";
        let sections = parse_markdown_sections(md);
        // Expect one section under H2 (the H1-only body is empty, flushed as nothing).
        let sec = sections.iter().find(|s| s.section.is_some()).unwrap();
        assert_eq!(sec.chapter.as_deref(), Some("Chapter One"));
        assert_eq!(sec.section.as_deref(), Some("Section A"));
        assert!(sec.text.contains("Prose under section"));
    }

    #[test]
    fn parse_markdown_second_h1_resets_section() {
        let md = "# Chapter One\n\n## Section A\n\nProse.\n\n# Chapter Two\n\nNew chapter.\n";
        let sections = parse_markdown_sections(md);
        // Last section should belong to Chapter Two with no section.
        let last = sections.last().unwrap();
        assert_eq!(last.chapter.as_deref(), Some("Chapter Two"));
        assert_eq!(last.section, None);
    }

    #[test]
    fn parse_markdown_prose_before_any_heading() {
        let md = "No heading here.\n\nJust some text.\n";
        let sections = parse_markdown_sections(md);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].chapter, None);
        assert_eq!(sections[0].section, None);
        assert!(sections[0].text.contains("No heading here"));
    }

    #[test]
    fn parse_markdown_h3_text_folded_into_body() {
        let md = "# Chapter\n\n### Sub note\n\nBody text.\n";
        let sections = parse_markdown_sections(md);
        assert_eq!(sections.len(), 1);
        let text = &sections[0].text;
        assert!(
            text.contains("Sub note"),
            "H3 heading should appear in body"
        );
        assert!(text.contains("Body text"));
    }

    #[test]
    fn parse_markdown_inline_emphasis_yields_plain_text() {
        let md = "# Title\n\n**bold** and *italic* and `code`.\n";
        let sections = parse_markdown_sections(md);
        assert_eq!(sections.len(), 1);
        let text = &sections[0].text;
        assert!(text.contains("bold"));
        assert!(text.contains("italic"));
        assert!(text.contains("code"));
        // No asterisks or backticks should remain.
        assert!(!text.contains('*'));
        assert!(!text.contains('`'));
    }

    #[test]
    fn parse_markdown_empty_content_returns_no_sections() {
        let sections = parse_markdown_sections("");
        assert!(sections.is_empty());
    }

    #[test]
    fn parse_markdown_multiline_paragraph_joins_with_spaces() {
        // A soft break (single newline within a paragraph, no heading context)
        // should be normalised to a single space.
        let md = "# Title\n\nLine one\nLine two\n";
        let sections = parse_markdown_sections(md);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].text.contains("Line one Line two"));
    }

    #[tokio::test]
    async fn extract_md_reads_and_parses_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.md");
        std::fs::write(
            &path,
            "---\ntitle: Ignored\n---\n# Chapter One\n\nSome prose.\n\n## Section A\n\nMore prose.\n",
        )
        .unwrap();

        let (title, sections) = extract_md(&path).await.expect("extract_md should succeed");
        assert_eq!(title, "sample");
        assert!(sections.iter().any(|s| s.chapter.as_deref() == Some("Chapter One")));
        assert!(sections
            .iter()
            .any(|s| s.section.as_deref() == Some("Section A")));
    }

    #[tokio::test]
    async fn extract_md_nonexistent_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = extract_md(&dir.path().join("nonexistent.md")).await;
        assert!(matches!(result, Err(IngestError::IoFailed(_))));
    }

    #[tokio::test]
    async fn extract_md_empty_document_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.md");
        std::fs::write(&path, "").unwrap();

        let result = extract_md(&path).await;
        assert!(matches!(result, Err(IngestError::ParseFailed(_))));
    }

    #[test]
    fn extract_heading_returns_none_for_no_match() {
        let html = "<p>no headings here</p>";
        assert!(extract_heading(html, "h1").is_none());
        assert!(extract_heading(html, "h2").is_none());
    }

    #[test]
    fn extract_heading_finds_h1() {
        let html = "<h1>Chapter One</h1><p>content</p>";
        assert_eq!(extract_heading(html, "h1").unwrap(), "Chapter One");
    }

    #[test]
    fn extract_heading_finds_h2() {
        let html = "<h2>Section A</h2><p>content</p>";
        assert_eq!(extract_heading(html, "h2").unwrap(), "Section A");
    }

    #[test]
    fn strip_html_tags_removes_all_tags() {
        let html = "<p>Hello <b>world</b>!</p>";
        assert_eq!(strip_html_tags(html), "Hello world!");
    }

    #[test]
    fn strip_html_tags_handles_empty() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn strip_html_tags_handles_no_tags() {
        assert_eq!(strip_html_tags("plain text"), "plain text");
    }

    #[test]
    fn strip_html_tags_normalizes_whitespace() {
        let html = "<div>  lots   of   spaces  </div>";
        assert_eq!(strip_html_tags(html), "lots of spaces");
    }
}
