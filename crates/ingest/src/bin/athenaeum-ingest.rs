//! athenaeum-ingest — CLI tool for bulk ingestion of PDF and EPUB files into the personal library.

use anyhow::{Context, Result};
use athenaeum_core::{Config, Engine};
use athenaeum_ingest::{discover_documents, ingest};
use clap::Parser;
use std::path::PathBuf;

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

#[cfg(not(tarpaulin_include))]
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
    let files = discover_documents(&args.directory, args.recursive)
        .context("Failed to discover documents")?;

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

        print!("[{}/{}] Ingesting {}... ", idx + 1, files.len(), file_name);
        std::io::Write::flush(&mut std::io::stdout()).ok();

        match ingest(&engine, file_path).await {
            Ok(summary) => {
                println!("✓ ({} chunks)", summary.chunks);
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
        println!("\nFailed Files:");
        for (file_name, error) in failed_files {
            println!("  - {}: {}", file_name, error);
        }
    }

    Ok(())
}
