# Decision Brief — Personal Knowledge Engine

*Sparring session · June 2026 · Supersedes: knowledge-engine-design-brief.md*

---

## Problem statement

Build a local-first semantic-search MCP server over a personal CS / FP / computer-graphics library (digital books + research papers). Returns cited, multi-source passages. Consumed by the brainstorm, spar, and planner agents in opencode as a research companion during thinking sessions — not during coding. Single user, single machine.

---

## Key decisions made

### Scope
- **One corpus: personal library only.** CS, functional programming, computer graphics books and research papers.
- **The work corpus (Yocto / BitBake manuals + problem/solution notes) is a separate, future project.** Not part of this build.
- **The Logbook (derived session notes) is deferred** — personal corpus first, session notes added later as a named feature; that experience then feeds the work corpus where it is required.

### Retrieval design
- **One dumb `search(query, k)` for the first roundtrip.** Plain vector similarity, multi-source results, each result cited with source + location.
- **No intent-specific retrieval at the backend.** Three consumers (brainstorm, spar, planner) have three different retrieval intents (extend/support · generate friction · factual lookup), but *intent lives in each agent's prompt*, not in the index. One tool, three prompts.
- **Stage (a) first: ranked cited passages; user synthesises.** Stage (b) — cross-source synthesis ("Book X and Book Y are the same result from different angles") — is a named, deferred feature to be built once (a) stabilises in daily use.

### Architecture
- **MCP-server-as-spine retained.** opencode agents speak MCP natively; this preserves a clean path to a future team service without redesigning retrieval.
- **Embedding: `nomic-embed-text` via Ollama.** Already in use, runs locally. Raw source text stored alongside every vector — re-embedding is a batch job, not an irreversible commitment.
- **Storage: LanceDB.** TypeScript client already validated with Bun.
- **Runtime: TypeScript + Bun.** No Python on the host.

### Consumers
- **Brainstorm agent** — uses corpus to extend and develop ideas; expand/support retrieval intent.
- **Spar agent** — uses corpus to generate friction and counter-arguments; intent handled in prompt, not backend.
- **Planner agent** — uses corpus for factual lookup when questions arise during planning.
- **Coding agents** — do **not** access the corpus.
- **Post-coding review agent** — may access the corpus; detail deferred.

---

## What was rejected, and why

| Rejected | Reason |
|---|---|
| FP ↔ BitBake structural-analogy bridging (Layer 1 + Layer 2 GraphRAG) | Non-shared vocabulary makes similarity-based bridging structurally blind to the very connections wanted. Not the real need for the personal corpus. |
| Team-graduation tier (Qdrant/pgvector, `access_level` routing, CI/git-PR ingestion, multi-writer) | Load-bearing only for the work/team story. Entirely cut from this project. |
| Topic-lens caching, active/archived lifecycle, lazy subgraph extraction | Over-engineered for a few-hundred-item personal corpus. Complexity ahead of a validated core. |
| Intent-specific dissent-retrieval for the spar agent | Requires more than cosine similarity; deferred alongside stage (b). |

---

## Risks to watch

1. **Similarity ≠ friction.** A pure-similarity backend feeds the spar agent material that *agrees* with the user. Prompt-side intelligence is the current mitigation. Watch in daily use whether the spar agent actually generates friction or becomes a confirmation machine.
2. **Embedding quality on technical/code-heavy content** with a general text embedding model is unproven on this corpus. Validate early with real queries before ingesting the full library.
3. **Scope creep relapse.** The predecessor brief got out of control by designing completeness ahead of a validated core. The fast-roundtrip discipline must hold. No feature ships before the one before it is in daily use.

---

## Deferred features (named, with triggers)

| # | Feature | Revisit trigger |
|---|---|---|
| D1 | Stage (b) cross-source synthesis | Stage (a) is in daily use; specific "I wish it had connected X and Y for me" moments identified |
| D2 | Dissent-retrieval for spar agent | Daily use shows the spar agent is systematically failing to generate friction from corpus results |
| D3 | Personal Logbook / session notes | Personal corpus is stable; concrete need to query own notes emerges |
| D4 | Work corpus (Yocto / BitBake) | Personal corpus experiment complete; Logbook feature built; team needs the system |
| D5 | Team graduation (shared store, multi-writer) | Work corpus prototype in daily personal use; at least one colleague actively wants access |

---

## Recommended build sequence

1. **Workspace spike** — verify `pdfjs-dist` + EPUB parsing library in Bun; keep as a permanent version-compatibility canary.
2. **Minimal MCP server** — one `search(query, k)` tool, LanceDB setup, Ollama embedding integration, raw source text stored alongside every vector.
3. **Ingestion pipeline** — EPUB + text-based PDF chunker (type: `reference`, 500–1000 tokens, overlap, preserves title/chapter/section/page for citation). Single `ingest(path)` tool.
4. **Validate on a small slice** — ingest 5–10 books/papers, run real queries, assess result quality *before* ingesting the full library.
5. **Wire agents** — add corpus `search()` call to brainstorm, spar, and planner agent prompts in opencode. Let each agent shape its own usage.
6. **Scale ingestion** — ingest remaining library once quality is confirmed.

---

## What this is not

- Not a coding assistant backend.
- Not a team tool (yet).
- Not a knowledge graph or relation-extraction system (yet).
- Not a proactive/always-on agent. All retrieval is explicitly triggered.
- Not Obsidian-dependent. Books and papers are the corpus; notes are deferred.

---

## Implementation note — language decision

The **Runtime** decision above (TypeScript + Bun) was reversed before any code was
written. The project is implemented in **Rust** (Cargo workspace). See
[ADR-0001](adr/0001-language-rust-over-typescript.md) for the full rationale.
