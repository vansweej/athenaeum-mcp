# athenaeum-mcp

A local-first semantic-search MCP server over a personal CS / FP / computer-graphics
library (digital books and research papers). Returns cited, multi-source passages.
Consumed by the brainstorm, spar, and planner agents in opencode as a research
companion during thinking sessions.

## What this is not

- Not a coding assistant backend — coding agents do not access the corpus.
- Not a team tool (yet).
- Not a knowledge graph or relation-extraction system (yet).
- Not proactive or always-on — all retrieval is explicitly triggered.
- Not Obsidian-dependent — books and papers are the corpus.

See [`docs/decision-brief.md`](docs/decision-brief.md) for the full scope,
architecture decisions, and deferred feature roadmap.

## Workspace layout

| Crate | Name | Role |
|---|---|---|
| `crates/core` | `athenaeum-core` | Ollama embedding + LanceDB storage + `search(query, k)` |
| `crates/ingest` | `athenaeum-ingest` | EPUB / PDF parse, chunk, cite |
| `crates/mcp-server` | `athenaeum-mcp-server` | `rmcp` binary — the MCP server spine |
| `crates/parser-spike` | `athenaeum-parser-spike` | Permanent pdfium + epub version canary |

## Prerequisites

- [Nix](https://nixos.org/) with flakes enabled
- [Ollama](https://ollama.com/) running locally with the `nomic-embed-text` model pulled:
  ```bash
  ollama pull nomic-embed-text
  ```

The book corpus lives on your local machine and is **never committed to this
repository** (see `.gitignore`).

## Development

All commands run inside the nix dev shell, which provides the Rust toolchain, pdfium,
and all other system dependencies that Cargo cannot supply.

```bash
# Enter the dev shell
nix develop

# Build
nix develop --command cargo build

# Test
nix develop --command cargo test

# Lint (zero warnings enforced)
nix develop --command cargo clippy -- -D warnings

# Format
nix develop --command cargo fmt

# Coverage (target ≥ 90%)
nix develop --command cargo tarpaulin

# Run the MCP server (scaffold — no tools registered yet)
nix develop --command cargo run -p athenaeum-mcp-server
```

## Architecture decision records

- [ADR-0001 — Rust over TypeScript + Bun](docs/adr/0001-language-rust-over-typescript.md)
