> **Superseded** by [docs/decision-brief.md](../decision-brief.md) — June 2026.

# Idea Brief — Personal Knowledge Engine

*Synthesized from interview · June 2026*

---

## What this is

A **local-first, two-source knowledge retrieval engine**, exposing three query faces via a standalone MCP server, that works with opencode today and graduates to Claude Code + a shared team service without redesign. It ingests a library of digital books (the **Library**) and derived coding-session notes (the **Logbook**), and serves encyclopedic lookup, deep collaborator reasoning, and topic-scoped knowledge maps — all scoped to whatever topic is currently being researched.

---

## The two systems and what they are not

The Library and the Logbook feel like two projects. They are not. They are **two sources feeding one retrieval core**, unified by the concept of an *active research topic*. The split is on the *writer* side only — books are ingested once, notes are written continuously. The reader side (embedding, retrieval, MCP tools) is shared.

### System A — the Library

| Attribute | Value |
|---|---|
| Corpus size | A few dozen to a few hundred books |
| Run target | Local machine only |
| Domains | **Personal research:** CS, functional programming, 3D graphics, HPC, CUDA, language design |
| | **Work:** DevOps, Yocto, BitBake, known problems + solutions |
| Cross-domain bridges | The personal↔work boundary is the highest-value discovery target: FP techniques ↔ BitBake DSL, language design ↔ Yocto patterns, HPC ↔ CUDA performance problems. These gaps are where "relations I can't see" live. |
| Legal constraint | Copyrighted books **never leave the local machine.** `access_level: personal` is non-negotiable for this source. |

### System B — the Logbook

| Attribute | Value |
|---|---|
| Capture model | **Derived, zero-effort.** opencode summarizes each coding session into a structured markdown file committed to the repo. No manual writing. |
| Structure | `problem → approach → solution → gotcha` |
| Home | Git-resident in the repo it belongs to |
| Trust/rot gate | Notes go through normal **PR review** before ingestion — existing workflow, zero new process |
| Access model | `access_level: team` — these are the entries that graduate to the shared store |
| Today's state | Handwritten fragments that rot quickly, plus knowledge trapped in colleagues' heads. This is the bus-factor problem the Logbook solves. |

---

## The single engine

```
 SOURCES                  CORE                      FACES (MCP tools)
 ──────────               ──────────────────────    ─────────────────────────
 Books (embed once)  →    Chunking (type-aware) →   search(query, scope, k)
                          Embeddings             →   deepen(live_context, topic)
 Session notes       →    Vector store           →   map(topic)
 (git-derived)            Topic lens (cached)    →   ingest(note_path)
                          Relation layers        →   relate(a, b)
```

Every face is a thin call over the same index. The agent host (opencode, Claude Code) does nothing except call tools and render results. All intelligence lives in the server.

---

## Decided architecture

### Embedding model — the one permanent commitment

**`nomic-embed-text` via Ollama.** Already in your setup. Open-source, runs locally, deployable as a shared service at team graduation without model change. This is the single irreversible choice — switching means re-embedding the entire corpus. Do not change it. **Always store the raw source text alongside every vector** so re-embedding is possible if ever needed; vectors without source text create permanent lock-in.

### Chunking — type-aware (Fork A → A2)

Dispatched on the `type` metadata field:

- **Books (type: reference):** recursive/semantic split on section → paragraph boundaries, 500–1000 tokens with overlap, preserving `title / chapter / section / page` for citation.
- **Notes (type: derived-solution | decision):** one note = one chunk (never split mid-solution), smaller and denser, with `repo / commit / session-id` provenance.

The schema field that was designed for portability also routes chunking. One schema, many jobs.

### The active-topic lens — persistent, lazy (Fork B → B2)

A topic starts as a query (ephemeral, free). The first time you invoke `map()` or `deepen()` on it, the server names the topic, extracts a relation subgraph **for that neighborhood only**, caches it, and reuses it for subsequent queries. You pay the extraction cost once per topic you dwell in — never for the whole library. This is the only locally-feasible model for relation discovery at your corpus size.

### Relation discovery layers — build C1, design for C2 (Fork C)

**Layer 0 — Vector similarity.**
Baseline. Finds things that are already alike. Built in everywhere; no extra work.

**Layer 1 — Cross-domain bridging.** ← *Build this first.*
Query retrieves top-k, then a second pass filters for pairs where `source-cluster(a) ≠ source-cluster(b)` — semantically close but from different domains (research book ↔ work solution). This is where your corpus topology pays off and where the majority of "I never connected those" moments come from. Cheap, fully local, no LLM extraction. Shockingly effective.

**Layer 2 — Lazy GraphRAG per topic subgraph.** ← *Design schema for it now, build when C1 is validated.*
LLM extracts entities + relations for the retrieved neighborhood only; multi-hop traversal then surfaces indirect links (A→B, B→C where A and C never co-occur). Delivers 2-hop "I could not have found this manually" discoveries. Pays one local LLM extraction pass per new topic, cached permanently.

### The three faces

**Mode (i) — Encyclopedic lookup → `search(query, scope, k)`**
Standard RAG. Embed query, retrieve top-k with metadata, cite source. Used constantly, for any topic. No surprises here — just make it fast and citation-rich.

**Mode (ii) — Collaborator → `deepen(live_context, topic)`**
The host sends a summary of the current conversation thread. The server embeds that context, retrieves a *wider neighborhood than what's visible in the chat window*, runs Layer 1/2 to find connections the conversation has not yet surfaced, and returns *"deeper background + two bridges you're not considering."* This is the "collaborator with understanding deeper than the current context" behavior — mechanically it is `search` + relation layers pointed at the live session instead of a typed query.

**Mode (iii) — Knowledge map → `map(topic)`**
Returns the cached topic subgraph as structured JSON (nodes, edges, weights). A thin viewer renders it — this is the only face requiring anything outside the agent. Scoped to the active research topic by design; never the whole library.

### Metadata schema — the portability contract

Design this now and never break it. Team-scale fields are included from day one even while running solo.

```
source        — book(title/author/page) | repo/commit/session-id
type          — reference | derived-solution | decision
topic         — tag(s) for the active lens, multi-value
provenance    — origin path + timestamp
access_level  — personal | team | public
author        — your name now; whoever on the team later
```

`access_level` is the load-bearing field for the hybrid architecture — it is the per-chunk router between what stays local and what graduates to the shared store.

### Hybrid local/shared architecture — the graduation model

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

The portability layers in order of permanence:

```
Permanent core     — embedding model, metadata schema, raw source text
Swappable inside   — storage engine (LanceDB → Qdrant, behind server walls)
Portable interface — MCP server (both hosts speak MCP natively)
Disposable clients — opencode skills → Claude Code agents
```

Invest in exactly the MCP boundary. Do not invest in an internal storage-abstraction layer during the prototype — that is complexity with no current payoff. Swap the storage glue when graduation actually happens.

### Ingest trigger — personal prototype uses D1

`git commit / PR merge of knowledge-*.md → re-embed changed notes → update local index.`

This matches the team graduation model exactly. A session hook (D2) is faster for solo feedback but skips the review gate you will want at team scale. Start D1 and keep it.

---

## Open questions — resolved before `plan`

These are decisions the interview surfaced but did not close. Resolve them before handing to the planner.

| # | Question | Why it matters |
|---|---|---|
| OQ1 | **Trust / rot signal.** How do you know a derived note is still true 6 months later? (A timestamp + a "last-confirmed" field? A CI check that flags notes older than N days?) | Without this, the Logbook becomes confident noise over time — the same rot problem as hand-written docs, just slower. |
| OQ2 | **Book ingestion scope.** Do you ingest the full text of every book up front, or ingest on demand (you add a book to a research topic and it gets chunked then)? | Up-front = one overnight job, full recall. On-demand = faster start, but partial index. |
| OQ3 | **Topic lifecycle.** When does a topic "close"? Does its cached subgraph persist forever, or get evicted? | Affects local disk growth and whether stale subgraphs mislead future queries. |
| OQ4 | **Team onboarding gate.** When the prototype graduates, who decides a note is `access_level: team` vs. `personal`? The author? The PR reviewer? Auto-tagged by repo? | Left unresolved, the team store fills with personal scratchpad noise. |
| OQ5 | **Visualization tooling for mode (iii).** Obsidian graph view over vault files? A standalone local viewer over graph JSON? Something else? | Low architectural urgency, but needed before mode (iii) is usable. |

---

## Prior art — what exists and what to learn from it

| Project | What it shows |
|---|---|
| [`enquire-mcp`](https://github.com/oomkapwn/enquire-mcp) | Hybrid BM25 + embeddings + reranking + HNSW over an Obsidian vault. The gold-standard retrieval architecture for a vault-as-corpus setup. Study the hybrid retrieval and reranking approach before finalizing `search()`. |
| [`opencode-obsidian-knowledge-workflow`](https://github.com/r007b34r/opencode-obsidian-knowledge-workflow) | 7 OpenCode skills wired to an Obsidian MCP server. Works with both opencode and Claude Code. Proof that the "thin skill, fat MCP server" pattern is viable in practice. |
| [`MegaMem`](https://github.com/C-Bjorn/MegaMem) | Obsidian vault → knowledge graph (neo4j / graphiti). Closest analog to Layer 2 GraphRAG. Look at how they handle entity extraction noise. |
| [`vault-cortex`](https://github.com/aliasunder/vault-cortex) | Plugin-free vault MCP server with FTS5 + LightRAG. Shows that Obsidian-the-app is optional — you can index plain markdown files directly, which is exactly your git-resident note model. |

---

## What this is not

- **Not Obsidian-dependent.** Your notes are plain markdown in git. Any MCP server that can index a directory of markdown files works. Obsidian could be a viewer for the Logbook, but it is not required infrastructure.
- **Not a RAG chatbot.** The goal is *relation discovery* and *compounding personal knowledge*, not a generic QA layer over documents. The retrieval engine serves that goal; resist scope creep toward a general assistant.
- **Not a documentation system.** The Logbook is a retrieval index, not documentation for other people. Notes do not need to be well-written; they need to be queryable and have accurate provenance.

---

## Handoffs

### → `spar` (before `plan`)

Challenge these assumptions before committing to a build:

1. **"Derived automatically" survives contact with reality.** Every no-effort-capture system ever built has a hidden cost that only appears after week three. What is it here?
2. **The "AI colleague" framing.** Your team is hyped about it — but do they want a queryable knowledge base or do they want an agent that answers questions in chat? These are different products with different trust requirements.
3. **Layer 1 cross-domain bridging is enough.** Is "semantically close but from different clusters" actually the surprise you want, or do you need the full 2-hop graph from the start to justify the build?

### → `plan` (after `spar` validates the approach)

Design the following in sequence:

1. **MCP server skeleton** — tool signatures, schema, LanceDB setup, Ollama embedding integration.
2. **Ingestion pipeline** — book chunker (type: reference), session-note chunker (type: derived-solution), `ingest()` tool, git-hook trigger.
3. **Retrieval core** — `search()` with Layer 0+1, the persistent topic lens, `relate()`.
4. **Collaborator face** — `deepen()` tool, live-context embedding, wider-neighborhood retrieval.
5. **Layer 2 lazy GraphRAG** — entity extraction, subgraph caching, `map()` JSON output.
6. **opencode skill wiring** — thin prompt wrappers calling each MCP tool, one per face.
7. **Graduation plan** — team deployment model, shared store setup, Claude Code agent translations.

---

## One-paragraph summary

You are building a local MCP server that embeds a library of technical books and auto-derived coding-session notes into a single vector index, unified by a persistent research-topic lens. It exposes five tools — encyclopedic search, collaborator deepening over your live context, topic knowledge map, note ingestion, and explicit relation bridging — that work identically under opencode now and Claude Code when the team graduates. The storage engine is swappable behind the server's walls; the embedding model and metadata schema are permanent. The Logbook is written by opencode session summaries committed to git and reviewed by PR — zero marginal effort, review-gated, already team-compatible. The highest-value discovery mechanism is cross-domain bridging between your research shelf and your work shelf, two domains close enough to share vocabulary but distinct enough to hide non-obvious analogies. Everything else is scaffolding around that one insight.
