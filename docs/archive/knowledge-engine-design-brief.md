> **Superseded** by [docs/decision-brief.md](../decision-brief.md) — June 2026.

# Personal Knowledge Engine — Design Brief

*Synthesized from idea brief + design grilling · June 2026*

---

## About this document

This is the **living design document** for the Personal Knowledge Engine. It serves as the single source of truth for architecture decisions, build scope, and feature roadmap.

**Conventions:**
- ✅ **Implemented** — feature is built and in use
- 🔜 **Deferred** — deliberately out of scope for now; revisit trigger noted
- *(no marker)* — in scope for current build

A [Future Features](#future-features) section at the bottom collects everything flagged as deferred, with the trigger for revisiting each.

---

## What this is

A **local-first, two-source knowledge retrieval engine**, exposing three query faces via a standalone MCP server, that works with opencode today and graduates to Claude Code + a shared team service without redesign. It ingests a library of digital books (the **Library**) and derived coding-session notes (the **Logbook**), and serves encyclopedic lookup, deep collaborator reasoning, and topic-scoped knowledge maps — all scoped to whatever topic is currently being researched.

---

## The two systems

The Library and the Logbook are **two sources feeding one retrieval core**, unified by the concept of an *active research topic*. The split is on the *writer* side only — books are ingested once, notes are written continuously. The reader side (embedding, retrieval, MCP tools) is shared.

### System A — the Library

| Attribute | Value |
|---|---|
| Corpus size | A few dozen to a few hundred books |
| Format | 50/50 PDF (text-based, no scanned) and EPUB |
| Run target | Local machine only |
| Domains | **Personal research:** CS, functional programming, 3D graphics, HPC, CUDA, language design |
| | **Work:** DevOps, Yocto, BitBake, known problems + solutions |
| Cross-domain bridges | The personal↔work boundary is the highest-value discovery target: FP techniques ↔ BitBake DSL, language design ↔ Yocto patterns, HPC ↔ CUDA performance problems. |
| Legal constraint | Copyrighted books **never leave the local machine.** `access_level: personal` is non-negotiable for this source. |
| Ingestion scope | **Hybrid** — batch ingest current library now, first-class path to add books later via CLI or `ingest()` tool |

### System B — the Logbook

| Attribute | Value |
|---|---|
| Capture model | **Derived, zero-effort.** opencode dumps a session note as markdown to disk; you pick it up and commit it. No rigid format required at write time. |
| Note types | `design-brief`, `session-log`, `gotcha` — each with its own chunking rule (see Chunking) |
| Home | Git-resident in the repo it belongs to |
| Trust/rot gate | Notes go through normal **PR review** before ingestion — batched weekly, not per session |
| Access model | `access_level: team` by default for team-repo notes — these graduate to the shared store |
| Today's state | Manually triggered session dumps, variable structure. Ingestion pipeline normalizes on the way in. |

---

## The single engine

```
 SOURCES                  CORE                      FACES (MCP tools)
 ──────────               ──────────────────────    ─────────────────────────
 Books (embed once)  →    Chunking (type-aware) →   search(query, scope, k)
                          Embeddings             →   deepen(live_context, topic)
 Session notes       →    Vector store           →   map(topic)
 (git-derived)            Topic lens (cached)    →   ingest(note_path | book_path)
                          Layer 1 cross-domain   →   relate(a, b)  🔜
```

Every face is a thin call over the same index. The agent host (opencode, Claude Code) does nothing except call tools and render results. All intelligence lives in the server.

---

## Architecture decisions

### Runtime

**TypeScript + Bun.** The MCP TypeScript SDK is the reference implementation. LanceDB TypeScript client is already validated with Bun. `pdfjs-dist` and an EPUB parsing library are to be verified in a workspace spike (kept permanently as a version compatibility canary).

Python is **not used on the host**. If a Python library proves necessary (e.g., fallback PDF extraction), it runs in an isolated container only — never touching the host system or Python environment.

### Embedding model — the one permanent commitment

**`nomic-embed-text` via Ollama.** Already in use. Open-source, runs locally, deployable as a shared service at team graduation without model change. This is the single irreversible choice — switching means re-embedding the entire corpus. **Always store the raw source text alongside every vector** so re-embedding is possible if needed.

### Chunking — type-aware

Dispatched on the `type` metadata field:

| Type | Rule |
|---|---|
| `reference` (books) | Recursive/semantic split at section → paragraph boundaries, 500–1000 tokens with overlap. Preserves `title / chapter / section / page` for citation. |
| `design-brief` | Split at header/section boundaries. Value is in decisions and open questions — preserve those as atomic chunks. |
| `session-log` | One note = one chunk. Never split mid-session. |
| `gotcha` | One note = one chunk. Single sharp fact, kept dense. |

### Book extraction

- **EPUB** — primary path, chapter boundaries explicit in structure.
- **PDF (text-based)** — `pdfjs-dist` via workspace spike. Code blocks from PDFs accepted as noisier than EPUB; no special compensation.
- **Scanned PDFs** — excluded from scope entirely.

### Staleness / rot signal

Notes carry `created_at` and `last_modified_at` from git. Retrieval tools attach a staleness annotation (e.g., `⚠ last modified 8 months ago`) to results older than a configurable threshold. Staleness is **informational only — never penalized in scoring.** A stale note that is still correct should not disappear from results.

### The active-topic lens — persistent, lazy

A topic starts as a query (ephemeral, free). The first time `map()` or `deepen()` is invoked on it, the server names the topic, extracts a relation subgraph **for that neighborhood only**, caches it, and reuses it for subsequent queries. Cost is paid once per topic you dwell in — never for the whole library.

**Topic lifecycle — active/archived:**
- *Active* — subgraph in hot cache, loaded at startup.
- *Archived* — subgraph kept on disk, not loaded at startup. Revival is one command.
- Explicit archiving is required; no TTL-based eviction. Topic state is a first-class metadata field.

### Relation discovery layers

**Layer 0 — Vector similarity.**
Baseline. Built in everywhere.

**Layer 1 — Cross-domain bridging.** ← *Build this first.*
Top-k retrieval, then a second pass filters for pairs where `source-cluster(a) ≠ source-cluster(b)` — semantically close but from different domains. Cheap, fully local, no LLM extraction.

**Layer 2 — Lazy GraphRAG per topic subgraph.** 🔜
LLM extracts entities + relations for the retrieved neighborhood only. Multi-hop traversal surfaces indirect links. Schema designed now, built after Layer 1 is validated in daily use.

### The three faces (MCP tools)

**`search(query, scope, k)` — Encyclopedic lookup**
Standard RAG. Embed query, retrieve top-k with metadata and staleness annotation, cite source. Fast, citation-rich.

**`deepen(live_context, topic)` — Collaborator**
Host sends a summary of the current conversation thread. Server embeds that context, retrieves a wider neighborhood than what's visible in the chat, runs Layer 1 to find connections the conversation has not yet surfaced, returns deeper background + cross-domain bridges. Mechanically: `search` + relation layers pointed at live session context.

**`map(topic)` — Knowledge map**
Returns the cached topic subgraph as structured JSON (nodes, edges, weights). A browser-based local viewer (Cytoscape.js or D3) renders it — generated by the server as a side-effect and auto-opened. JSON schema is expressive enough to translate to Obsidian graph format later (see Future Features).

**`ingest(path, type?)` — Ingestion**
Accepts a book path or a note path. Detects format (EPUB/PDF/markdown), dispatches to the appropriate chunker, embeds, and updates the index. Handles both the batch-ingest-now and add-later cases.

**`relate(a, b)` — Explicit relation bridging** 🔜
Deferred. See Future Features.

### Metadata schema — the portability contract

Design now, never break it. Team-scale fields included from day one.

```
source        — book(title/author/page) | repo/commit/session-id
type          — reference | design-brief | session-log | gotcha
topic         — tag(s) for the active lens, multi-value
topic_state   — active | archived
provenance    — origin path + timestamp
access_level  — personal | team | public
author        — your name now; whoever on the team later
created_at    — from git
last_modified_at — from git
```

`access_level` is the load-bearing field for the hybrid architecture — it is the per-chunk router between what stays local and what graduates to the shared store.

### Access level — team gate

| Rule | Detail |
|---|---|
| Default | Repo-of-origin is the primary router: note committed to a team repo → `access_level: team` |
| Override | Author or PR reviewer can explicitly set `access_level: personal` for notes in a team repo that should not graduate |
| Books | Always `access_level: personal`, non-negotiable |

### Visualization — `map()` output

**Phase 1 (now):** Browser-based local viewer. Minimal self-contained HTML with Cytoscape.js or D3. Generated by the MCP server as a side-effect of `map()`, auto-opened. Zero new infrastructure.

**Phase 2 (later):** 🔜 Obsidian graph view. See Future Features. JSON schema is forward-compatible.

### Hybrid local/shared architecture — graduation model

```
 Personal (now)                     Team (later)
 ─────────────────────────          ─────────────────────────────────────
 Local LanceDB on disk              Hosted Qdrant or pgvector
 access_level: personal (books)  →  stays local, never synced
 access_level: team (notes)      →  synced to shared store
 opencode client                    Claude Code clients (all team)
 Single writer                      Multi-writer via git/PR ingestion
 Local Ollama embeddings            Shared embedding service (same model)
```

The shared team index is built from git: every repo carries its knowledge markdown → CI re-embeds changed notes → shared store updates. The review gate is the PR. No new tooling; the team's normal workflow is the contribution pipeline.

### MCP server as the portable spine

**All retrieval logic lives inside the MCP server.** opencode skills and Claude Code agents are thin prompt wrappers that call MCP tools. When the team switches to Claude Code, the migration is rewriting a handful of markdown skill/agent definitions — not the retrieval engine.

```
Permanent core     — embedding model, metadata schema, raw source text
Swappable inside   — storage engine (LanceDB → Qdrant, behind server walls)
Portable interface — MCP server (both hosts speak MCP natively)
Disposable clients — opencode skills → Claude Code agents
```

### Ingest trigger

`git commit / PR merge of knowledge-*.md → re-embed changed notes → update local index.`
Notes batched into one PR per week. Matches the team graduation model exactly.

---

## Build sequence

Hand to `/plan` in this order:

1. **Workspace spike** — verify `pdfjs-dist` + EPUB library in Bun; keep as permanent version canary.
2. **MCP server skeleton** — tool signatures, metadata schema, LanceDB setup, Ollama embedding integration.
3. **Ingestion pipeline** — book chunker (PDF + EPUB, type: reference), note chunker (design-brief / session-log / gotcha), `ingest()` tool, git-hook trigger.
4. **Retrieval core** — `search()` with Layer 0 + Layer 1 cross-domain bridging, persistent topic lens, active/archived lifecycle.
5. **Collaborator face** — `deepen()` tool, live-context embedding, wider-neighborhood retrieval.
6. **Knowledge map face** — `map()` JSON output, browser viewer, topic subgraph caching.
7. **opencode skill wiring** — thin prompt wrappers calling each MCP tool, one per face.
8. **Graduation plan** — team deployment model, shared store setup, Claude Code agent translations.

---

## Prior art

| Project | What it shows |
|---|---|
| [`enquire-mcp`](https://github.com/oomkapwn/enquire-mcp) | Hybrid BM25 + embeddings + reranking + HNSW over an Obsidian vault. Study the hybrid retrieval and reranking approach before finalizing `search()`. |
| [`opencode-obsidian-knowledge-workflow`](https://github.com/r007b34r/opencode-obsidian-knowledge-workflow) | 7 OpenCode skills wired to an Obsidian MCP server. Proof that the "thin skill, fat MCP server" pattern is viable in practice. |
| [`MegaMem`](https://github.com/C-Bjorn/MegaMem) | Obsidian vault → knowledge graph (neo4j / graphiti). Closest analog to Layer 2 GraphRAG. Look at entity extraction noise handling. |
| [`vault-cortex`](https://github.com/aliasunder/vault-cortex) | Plugin-free vault MCP server with FTS5 + LightRAG. Shows that Obsidian-the-app is optional — plain markdown files index directly, which is exactly the git-resident note model. |

---

## Future features

These are deliberately out of scope for the current build. Each has a trigger for revisiting.

| # | Feature | Description | Revisit trigger |
|---|---|---|---|
| F1 | **Layer 2 — Lazy GraphRAG** | LLM entity extraction + multi-hop graph traversal per topic subgraph. Surfaces 2-hop indirect connections (A→B, B→C) that Layer 1 cannot find. | Layer 1 is in daily use and the cross-domain bridges it finds are not sufficient — specific gaps identified. |
| F2 | **`relate(a, b)` tool** | Explicit relation bridging between two named concepts. Returns the shortest path and intermediate nodes between them in the topic subgraph. | Layer 2 is built; or a concrete use case emerges from Layer 1 usage that `search()` cannot satisfy. |
| F3 | **Proactive agent / always-on injection** | Background watcher that injects relevant knowledge into the conversation without being explicitly queried. | Team has adopted the knowledge base and explicitly requests proactive behavior; trust and accuracy requirements defined. |
| F4 | **Obsidian graph view for `map()`** | Export topic subgraph as Obsidian vault files and render in Obsidian graph view. `map()` JSON schema is forward-compatible with this. | Browser viewer is insufficient for the complexity of subgraphs you're working with. |
| F5 | **Team graduation — shared store** | Hosted Qdrant or pgvector, shared embedding service, CI-driven ingestion, Claude Code agent translations. | Prototype is in daily personal use and at least one colleague actively wants access. |

---

## What this is not

- **Not Obsidian-dependent.** Notes are plain markdown in git. Obsidian could be a viewer, but it is not required infrastructure.
- **Not a RAG chatbot.** The goal is *relation discovery* and *compounding personal knowledge*, not a generic QA layer over documents.
- **Not a documentation system.** The Logbook is a retrieval index, not documentation for other people. Notes do not need to be well-written; they need to be queryable and have accurate provenance.
- **Not a proactive agent** (yet). All retrieval is explicitly triggered. The human stays in the decision loop.
