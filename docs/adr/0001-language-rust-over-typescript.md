# ADR-0001 — Rust over TypeScript + Bun

*Date: June 2026 · Status: Accepted*

---

## Context

The Decision Brief (`docs/decision-brief.md`) chose **TypeScript + Bun** as the
runtime on the basis of:

1. The LanceDB TypeScript client had already been validated under Bun.
2. An explicit "no Python on the host" constraint was non-negotiable.

No code had been written when the language choice was reopened.

## Decision

Build the project in **Rust** on a Cargo workspace.

**Rationale:**

- The `lancedb` Rust crate is the *native core*; the TypeScript client is a binding
  over it. The Rust path is closer to the metal and removes a binding layer.
- The official MCP SDK (`rmcp`) has a first-class Rust implementation.
- Citation-grade PDF text extraction (`pdfium-render`) wraps the same pdfium library
  used in production PDF tools; quality is higher than pure-JS alternatives.
- The `epub` crate provides pure-Rust EPUB parsing with no system dependencies.
- The "no Python" constraint is honoured more strictly: no Node.js runtime either.
- Native `Result<T, E>` + `thiserror` + `?` operator maps directly to the
  Result-pattern mandate in the project's coding standards.
- The nix dev shell absorbs the one real cost: the pdfium native shared library.

## Consequences

- **Accepted costs:** `rmcp` and `lancedb` are younger than their TypeScript
  counterparts; versions must be pinned and the parser-spike canary validates the
  toolchain on every upgrade.
- **Nix is now required** for development (pdfium native lib, protoc for the lance
  build). The dev shell in `flake.nix` provides all non-cargo dependencies.
- **Coverage** runs via `cargo tarpaulin` (works on macOS); target ≥ 90%.
- The brief's build-sequence notes (steps 2–6) were written with TypeScript idioms;
  read them through a Rust lens in subsequent planning sessions.
