use crate::error::IngestError;
use epub::doc::EpubDoc;
use pdfium_render::prelude::*;
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
