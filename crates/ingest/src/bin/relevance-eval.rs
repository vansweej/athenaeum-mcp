//! Relevance eval — hand-run search-quality instrument.
//!
//! Ingests a corpus directory, runs a set of queries from a TOML file, and
//! prints full passages for human judgment. This is NOT a pass/fail test —
//! it is a diagnostic that produces evidence for a human verdict.
//!
//! # Usage
//!
//! ```text
//! nix develop --command cargo run -p athenaeum-ingest --bin relevance-eval -- \
//!     --corpus /path/to/corpus
//! ```
//!
//! # Preconditions
//!
//! - Live Ollama running with `nomic-embed-text` loaded.
//! - Run from the repo root (default store is `./data/athenaeum`).
//! - The `--corpus` directory must contain `.pdf` and/or `.epub` files.
//!
//! # Output
//!
//! For each query, prints: the query text, its `expect` tag, the `note`,
//! and then the top-k hits each with `score`, `source`, `location`, and
//! the **full** passage `text`. The operator reads the output and records
//! a Green/Yellow/Red verdict (see `docs/relevance-eval.md`).
//!
//! # CI
//!
//! This binary MUST NOT be added to CI or the default `cargo test` suite.
//! It requires live Ollama and a human grader.

use anyhow::{Context, Result};
use athenaeum_core::{Config, Engine};
use athenaeum_ingest::discover_documents;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "relevance-eval")]
#[command(about = "Evaluate search quality on a given corpus with a list of queries")]
struct Args {
    /// Directory containing PDF/EPUB files to ingest for the eval.
    #[arg(long, value_name = "DIR")]
    corpus: PathBuf,

    /// Path to the queries TOML file.
    #[arg(long, default_value = "crates/ingest/eval/queries.toml")]
    queries: PathBuf,

    /// Number of top results to return per query.
    #[arg(long, default_value_t = 5)]
    k: usize,
}

struct EvalQuery {
    text: String,
    expect: String, // "hit" or "miss"
    note: String,
}

#[cfg(not(tarpaulin_include))]
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if !args.corpus.is_dir() {
        anyhow::bail!("Corpus directory does not exist: {}", args.corpus.display());
    }

    // Load queries
    let query_content = std::fs::read_to_string(&args.queries)
        .with_context(|| format!("Failed to read queries file: {}", args.queries.display()))?;
    let parsed: toml::Value =
        toml::from_str(&query_content).context("Failed to parse queries TOML file")?;

    let queries: Vec<EvalQuery> = parsed
        .get("query")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| EvalQuery {
                    text: item
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("MISSING")
                        .to_string(),
                    expect: item
                        .get("expect")
                        .and_then(|v| v.as_str())
                        .unwrap_or("hit")
                        .to_string(),
                    note: item
                        .get("note")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    println!(
        "Loaded {} query/queries from {}",
        queries.len(),
        args.queries.display()
    );
    println!("Corpus directory: {}", args.corpus.display());
    println!("Top-k: {}", args.k);
    println!();

    // ── Initialize engine ──────────────────────────────────────────────────
    let config = Config::default();
    let engine = Engine::new(config)
        .await
        .context("Failed to initialize engine")?;

    // ── Collect files ──────────────────────────────────────────────────────
    let files = discover_documents(&args.corpus, true).context("Failed to discover documents")?;

    println!(
        "Ingesting {} file(s) from {} ...\n",
        files.len(),
        args.corpus.display()
    );

    let mut ingest_ok = 0usize;
    let mut ingest_fail = 0usize;

    for (idx, file_path) in files.iter().enumerate() {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        print!("  [{}/{}] {} ... ", idx + 1, files.len(), file_name);
        std::io::Write::flush(&mut std::io::stdout()).ok();

        match athenaeum_ingest::ingest(&engine, file_path).await {
            Ok(summary) => {
                println!("✓ ({} chunks)", summary.chunks);
                ingest_ok += 1;
            }
            Err(e) => {
                println!("✗ ERROR: {}", e);
                ingest_fail += 1;
            }
        }
    }

    println!("\nIngest done: {} ok, {} failed\n", ingest_ok, ingest_fail);
    println!("{}", "=".repeat(72));

    // ── Run queries ────────────────────────────────────────────────────────
    println!("\nSearch Results\n");

    for (query_idx, q) in queries.iter().enumerate() {
        println!(
            "═══ Query {}/{} ════════════════════════════════════════════════",
            query_idx + 1,
            queries.len()
        );
        println!("  Query: {}", q.text);
        println!("  Expect: {}", q.expect);
        println!("  Note:   {}", q.note);
        println!();

        match engine.search(&q.text, args.k).await {
            Ok(hits) => {
                if hits.is_empty() {
                    println!("  (no results returned)");
                } else {
                    for (hit_idx, hit) in hits.iter().enumerate() {
                        println!(
                            "  ── Hit {}/{} ────────────────────────────────────────",
                            hit_idx + 1,
                            hits.len()
                        );
                        println!("  Score:    {:.4}", hit.score);
                        println!("  Source:   {}", hit.source);
                        println!("  Location: {}", hit.location);
                        println!("  Text:");
                        for line in hit.text.lines() {
                            println!("    {}", line);
                        }
                        println!();
                    }
                }
            }
            Err(e) => {
                println!("  ⚠ SEARCH FAILED: {}", e);
            }
        }

        // Grading reminder
        println!("  ── Grade ──────────────────────────────────────────────");
        println!("  1. Relevant passage present in top-{}? yes / no", args.k);
        println!("  2. Passage coherent (not mid-sentence / not truncated)? yes / no");
        if q.expect == "miss" {
            println!("  3. (MISS) All scores low (< 0.5)? yes / no");
            println!(
                "  3b. (MISS) Any hit that looks confidently relevant? no / yes → DEMO-KILLER"
            );
        }
        println!();
    }

    // Grading summary footer
    println!("{}", "=".repeat(72));
    println!("\n All queries complete. Grade each query above, then record the overall verdict:\n");
    println!("  GREEN  = coherent, relevant, misses low-score  → ship dedup, skip Fork A");
    println!("  YELLOW = right sources but degraded text        → fix Threat 2 only");
    println!("  RED    = incoherent / confident lies on misses  → fix Threats 1+2 before demo");
    println!("\nRecord the verdict and any findings in your eval notes.");

    Ok(())
}
