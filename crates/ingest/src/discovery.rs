//! Document discovery utilities — recursively collect supported files from a directory.

use std::path::{Path, PathBuf};

use crate::error::IngestError;

/// Recursively collect `.pdf`, `.epub`, `.md`, and `.markdown` files under `dir`, returned sorted.
///
/// When `recursive` is `false`, subdirectories are ignored. Extension matching
/// is case-insensitive. I/O failures are mapped to [`IngestError::IoFailed`].
pub fn discover_documents(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>, IngestError> {
    let mut files = Vec::new();

    let entries = std::fs::read_dir(dir).map_err(|e| IngestError::IoFailed(e.to_string()))?;

    for entry in entries {
        let entry = entry.map_err(|e| IngestError::IoFailed(e.to_string()))?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if let Some(ext_str) = ext.to_str() {
                    let ext_lower = ext_str.to_lowercase();
                    if ext_lower == "pdf"
                        || ext_lower == "epub"
                        || ext_lower == "md"
                        || ext_lower == "markdown"
                    {
                        files.push(path);
                    }
                }
            }
        } else if path.is_dir() && recursive {
            let mut subdir_files = discover_documents(&path, recursive)?;
            files.append(&mut subdir_files);
        }
    }

    // Sort files for consistent ordering
    files.sort();

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn discovers_pdf_and_epub_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("doc1.pdf"), "pdf content").unwrap();
        fs::write(dir.path().join("doc2.epub"), "epub content").unwrap();
        fs::write(dir.path().join("notes.txt"), "text").unwrap();
        fs::write(dir.path().join("data"), "no extension").unwrap();

        let files = discover_documents(dir.path(), false).unwrap();
        assert_eq!(files.len(), 2);

        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"doc1.pdf".to_string()));
        assert!(names.contains(&"doc2.epub".to_string()));
    }

    #[test]
    fn discovers_markdown_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("notes.md"), "# notes").unwrap();
        fs::write(dir.path().join("readme.markdown"), "# readme").unwrap();
        fs::write(dir.path().join("skip.txt"), "text").unwrap();

        let files = discover_documents(dir.path(), false).unwrap();
        assert_eq!(files.len(), 2);

        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"notes.md".to_string()));
        assert!(names.contains(&"readme.markdown".to_string()));
        assert!(!names.contains(&"skip.txt".to_string()));
    }

    #[test]
    fn case_insensitive_extension_matching() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("DOC.PDF"), "content").unwrap();
        fs::write(dir.path().join("Book.EPUB"), "content").unwrap();
        fs::write(dir.path().join("mixed.Pdf"), "content").unwrap();

        let files = discover_documents(dir.path(), false).unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn sorted_output() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("z.epub"), "z").unwrap();
        fs::write(dir.path().join("a.pdf"), "a").unwrap();
        fs::write(dir.path().join("m.epub"), "m").unwrap();

        let files = discover_documents(dir.path(), false).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["a.pdf", "m.epub", "z.epub"]);
    }

    #[test]
    fn non_recursive_ignores_subdirectories() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.pdf"), "nested").unwrap();
        fs::write(dir.path().join("root.pdf"), "root").unwrap();

        let files = discover_documents(dir.path(), false).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap().to_str().unwrap(), "root.pdf");
    }

    #[test]
    fn recursive_includes_subdirectory_files() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.pdf"), "nested").unwrap();
        fs::write(dir.path().join("root.pdf"), "root").unwrap();

        let files = discover_documents(dir.path(), true).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn nonexistent_directory_returns_error() {
        let err = discover_documents(Path::new("/nonexistent/path"), false).unwrap_err();
        assert!(matches!(err, IngestError::IoFailed(_)));
    }

    #[test]
    fn empty_directory_returns_empty_list() {
        let dir = tempdir().unwrap();
        let files = discover_documents(dir.path(), false).unwrap();
        assert!(files.is_empty());
    }
}
