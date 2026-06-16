//! athenaeum-ingest — CLI tool for bulk ingestion of PDF and EPUB files into the personal library.

use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Context, Result};
use clap::Parser;
use athenaeum_core::{Config, Engine};
use athenaeum_ingest::ingest;

#[derive(Parser, Debug)]
#[command(name = "athenaeum-ingest")]
#[command(about = "Bulk ingest PDF and EPUB files into the personal library", long_about = None)]
struct Args {
    /// Directory containing PDF and EPUB files to ingest
    #[arg(value_name = "DIR")]
    directory: PathBuf,

    /// Recursively ingest files in subdirectories
    #[arg(short, long)]
    recursive: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Validate directory exists
    if !args.directory.is_dir() {
        anyhow::bail!("Directory does not exist: {}", args.directory.display());
    }

    // Initialize the engine
    let config = Config::default();
    let engine = Engine::new(config)
        .await
        .context("Failed to initialize search engine")?;

    // Collect files to ingest
    let files = collect_files(&args.directory, args.recursive)?;

    if files.is_empty() {
        println!("No PDF or EPUB files found in {}", args.directory.display());
        return Ok(());
    }

    println!(
        "Found {} file(s) to ingest in {}",
        files.len(),
        args.directory.display()
    );

    // Ingest files
    let mut total_documents = 0;
    let mut total_chunks = 0;
    let mut failed_files = Vec::new();

    for (idx, file_path) in files.iter().enumerate() {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        print!(
            "[{}/{}] Ingesting {}... ",
            idx + 1,
            files.len(),
            file_name
        );
        std::io::Write::flush(&mut std::io::stdout()).ok();

        match ingest(&engine, file_path).await {
            Ok(summary) => {
                println!(
                    "✓ ({} chunks)",
                    summary.chunks
                );
                total_documents += summary.documents;
                total_chunks += summary.chunks;

                if args.verbose {
                    println!("  Path: {}", file_path.display());
                }
            }
            Err(e) => {
                println!("✗ Error: {}", e);
                failed_files.push((file_name.to_string(), e.to_string()));
            }
        }
    }

    // Print summary
    println!("\n{}", "=".repeat(60));
    println!("Ingestion Summary");
    println!("{}", "=".repeat(60));
    println!("Total files processed: {}", files.len());
    println!("Successful: {}", files.len() - failed_files.len());
    println!("Failed: {}", failed_files.len());
    println!("Total documents: {}", total_documents);
    println!("Total chunks: {}", total_chunks);

    if !failed_files.is_empty() {
        println!("\n{}", "Failed Files:".to_string());
        for (file_name, error) in failed_files {
            println!("  - {}: {}", file_name, error);
        }
    }

    Ok(())
}

/// Collect all PDF and EPUB files from the given directory.
fn collect_files(dir: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let entries = fs::read_dir(dir).context("Failed to read directory")?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if let Some(ext_str) = ext.to_str() {
                    let ext_lower = ext_str.to_lowercase();
                    if ext_lower == "pdf" || ext_lower == "epub" {
                        files.push(path);
                    }
                }
            }
        } else if path.is_dir() && recursive {
            let mut subdir_files = collect_files(&path, recursive)?;
            files.append(&mut subdir_files);
        }
    }

    // Sort files for consistent ordering
    files.sort();

    Ok(files)
}
