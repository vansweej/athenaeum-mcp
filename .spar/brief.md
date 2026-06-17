## Feature

Bulk-ingest hardening for athenaeum-mcp: per-document dedup/upsert (correctness)
plus a search-quality relevance eval that decides whether chunker hardening
(Fork A) is needed before the colleague demo. Parallelism is stripped from the
demo-critical path; it remains an honest later play.

## Project context — two targets, two motives (the keystone)

- Personal/local (now): single Ollama, little server-side concurrency. Throughput
  is whatever one process delivers; correctness + dedup matter, raw speed does not.
  Concurrency is INVISIBLE here (`--jobs 4` ≈ `--jobs 1`) — so the parallel feature
  cannot be validated on this hardware.
- Work (real target, deferred): budgeted Ollama with genuine parallelism; multi-hour
  ingests unacceptable; concurrency must be PROVEN before trusted.
- Two motives that do NOT share a priority order:
  1. Colleague-pull demo (ROI-bound): search responses that matter for their problem.
     The taste is search quality + robust ingest, NOT speed.
  2. Developer's play project (not ROI-bound): parallelism, for enjoyment; agents make
     it cheap to build. Legitimate driver, but must not be load-bearing for the demo.
- Firewall rule: project-2 (colleague) correctness must NOT depend on project-1
  (concurrency) experiments. Parallelism layers on top, flag-gated, default off.

## Key decisions made

- **Build order:** (1) dedup serial → (2) relevance eval / quality gate → (3) Fork A
  (chunker hardening, conditional on verdict) → [DEFERRED] parallelism.
- **Upsert = delete-then-add** (reuse `Store::add` verbatim, `store.rs:113-170`);
  NOT `merge_insert` (row-granular; needs 3-clause orphan-delete + reader plumbing).
- **`doc_id` = absolute file path.** doc_id is per-document, attached in `ingest()`
  (`ingest.rs:44`), not per-chunk.
- **Delete `add_passage` entirely;** migrate ~6 call sites to `add_passages`.
- **Hide `doc_id` from `SearchHit`** — search path frozen (`search.rs`, `store.rs:209-238`).
- **`--jobs` DEFAULTS TO 1** (changed from 4): the default path is the colleague/demo
  path and must be the proven serial one. High `--jobs` is explicit opt-in.
- **Parallelism stripped from the demo-critical path.** No Mutex, no channel, no
  embed/write seam split for the first delivery. The parallel path is deferred
  to honest later play, consistent with the firewall. The only durable code from
  the earlier brainstorm is a `FailingEmbedder` (≈5 lines against the existing
  `Embedder` trait, `embed.rs:18`) proving non-negotiable #2 survives the
  "keep going" loop.
- **Crawl parallelism REJECTED** — `read_dir` isn't the bottleneck, Ollama is.

## Two non-negotiables (correctness, not polish)

- SQL-quote the delete predicate: `doc_id` is a path; `O'Reilly - SICP.pdf` will
  appear; escape `'`→`''` or the filter breaks at runtime.
- No `?` inside `buffer_unordered`: one bad file must not abort the run; preserve
  "record failure, keep going" (`athenaeum-ingest.rs:80-83`). Match both arms.

## Search quality — the actual demo promise (NEW)

The colleague demo is sold on **search responses that matter for their problem.**
The search path is frozen (`store.rs:187-241`); it's honest but minimal — no
re-ranking, no filtering, no query-time dedup. That means **search quality is a
function of chunk quality**, and the chunker (`chunking.rs`) sets the ceiling.
Three concrete threats were surfaced during the brainstorm:

1. **Sentence splitter shreds technical prose** (`chunking.rs:91-138`): ends
   sentences on `.`/`!`/`?` + uppercase. Citations ("Fig. 3"), abbreviations
   ("e.g.", "i.e.", "vs."), decimals ("3.14"), and section numbers ("4.2")
   all produce false splits. For a technical audience — exactly the demo's
   target — this corrupts chunk boundaries in the most semantically harmful way.
2. **Token estimate is a guess** (`chunking.rs:141`): `words × 1.3`. BPE
   tokenizers (nomic-embed-text) encode technical text far denser. A "1000
   token" chunk may be 1500+ actual tokens → silent truncation in Ollama.
   Truncated content is **unsearchable** while appearing ingested. This is
   the unrecoverable failure (search cannot find absent text).
3. **`min_tokens` is dead code** (`chunking.rs:73-74`): the comment says it
   gates new-chunk creation, but only `> max_tokens` is ever checked. The
   config knob is inert — tuning it does nothing.

These threats are **unknown** — they may or may not fire on the actual corpus.
The correct first move is to measure before building.

## Fork B — relevance eval (next immediate step)

A hand-run evaluation against live Ollama, designed to **measure** search quality
on the existing code, without assuming any of Threats 1-3 fire. The verdict
decides whether Fork A (chunker hardening) is necessary.

### Design

- **Format:** a checked-in script or `#[ignore]`d binary (lives in `crates/ingest/src/bin/`
  or equivalent), runs inside `nix develop` with Ollama + `nomic-embed-text` up.
- **Corpus:** the two domain books already ingested (brief line 102 in the original).
  These are in **the shared work domain** — same domain as the colleague, not
  proxy. The colleague's specific books aren't ingested yet (corpus-build is
  deferred real work), but the domain and problem-framing are shared.
- **Queries:** 8-10, written **before re-reading the books** (the 5-6 year gap
  since last reading is the only defense against teaching-to-the-test; re-reading
  would undo it). Include 2-3 deliberate out-of-scope queries (things not in
  either book) to test whether the system correctly low-scores or confidently
  lies — the latter is a demo-killer.
- **Output:** for each query, print the top-5 hits with full passage `text`,
  source, location, and score. The deliverable is **human judgment recorded as
  text** — NOT a pass/fail `assert!`. Asserting relevance is false confidence.
- **Grading criteria:** for each query, record: (a) is a relevant passage present
  in top-k? (b) is the returned passage *coherent* (not starting mid-sentence or
  truncated)? (c) any high-score irrelevant hit (the "confident lie")?
- **Relationship to threats:** source-level grading ("right book in top-1") is
  NOT sufficient — Threats 1-2 corrupt *within*-book passage quality. The grader
  must read the returned `text` to detect shredded boundaries or truncated tails.

### Verdict (recorded, not asserted)

Three possible outcomes:

1. **Green:** all queries return coherent, relevant passages; out-of-scope queries
   score low; neither Threat 1 nor Threat 2 fires on this corpus. → **Chunker is
   adequate; ship dedup; Fork A deferred.**
2. **Yellow:** the right sources appear but passages are *degraded* (false splits,
   minor truncation). → **Threat 1 fires but is recoverable via overlap; Threat 2
   needs a conservative safety margin on `max_tokens`. Target Fork A at Threat 2
   only (the unrecoverable one).**
3. **Red:** passages are incoherent (shredded citations, severe truncation),
   or out-of-scope queries return confident lies. → **Demo-blocking. Fork A
   is required before colleague demo.**

### Ceiling of the eval

This eval validates: **"on my domain, with queries drawn from shared problem-framing,
the mechanism produces coherent relevant passages."** That IS the colleague's
question distribution (same person pop, same domain). What it DOES NOT validate:
**the colleague's specific corpus.** Their books aren't in the store. That gap
is known, deferred, and meaningful — but the mechanism is shared. If the mechanism
passes here, the *remaining* risk at demo time is corpus-specific: does their
particular text trigger corners of the chunker the eval books didn't?

### Fork B code cost

- One `FailingEmbedder` (≈5 lines against `Embedder` trait, `embed.rs:18`):
  returns `Err` on the Nth embed call; runs as an `#[ignore]`d test exercising
  non-negotiable #2.
- The eval script itself: reads query list, runs searches, prints results. No
  test-support feature flag needed (it's a binary, not a macro).
- The output is **not** wired into CI. It's a hand-run instrument.

## Fork A — chunker hardening (conditional, deferred to verdict)

Only built if Fork B returns Yellow (Threat 2 only) or Red (Threats 1+2).

- **Threat 2 priority** (silent truncation): swap `estimate_tokens` for a real
  BPE tokenizer or a conservative multiplier (e.g., `words × 2.0`). This is the
  unrecoverable failure; search cannot find content that was truncated before
  embedding.
- **Threat 1** (false sentence splits): harder. A real rule-based sentence
  segmenter (e.g., regex-based abbreviation list with lookahead exceptions)
  or delegate to a library. The threshold for "good enough" is "doesn't shred
  citations" — not perfect. Overlap (`chunking.rs:62`) partially mitigates
  bad splits; quantify the mitigation during Fork B.
- **`min_tokens`**: either wire it in or remove the dead comment/config field.
  Cosmetic, but traps the next developer.

## Rejected alternatives (from brainstorm; retired cleanly)

- **Sleep-based validation harness (concurrency correctness):** See brainstorm
  session 2026-06-16. `SleepEmbedder` peak-counter tested `futures::buffer_unordered`
  (a tautology); `CountingStore` writer peak required an uncosted `Store`-trait
  refactor and tested `tokio::Mutex` (not our code). The harness was elaborate
  scaffolding around a single test — `FailingEmbedder` — that needs neither sleep
  nor counters. Stripped entirely; the 5-line `FailingEmbedder` is the only survivor.
- **Mutex vs. channel for the writer:** Both were downstream of a parallel path
  that is now deferred. The writer design is irrelevant until parallelism is built.
- **`merge_insert` native upsert — row-granular vs. document-granular dedup;** more
  surface, no real atomicity gain at doc granularity. Still rejected (same reasoning).
- **`rm -rf ./data/athenaeum` as the dedup story — fine at 2 books, ABSURD at a
  few hundred PDFs / multi-hour ingest (re-embed 120k chunks to dedup 400).
  Still rejected.
- **Parallel directory crawl — bottleneck is Ollama, not `read_dir`. Still rejected.
- **Tuning `max_tokens` INSTEAD of parallelism — it's the cheaper local-speed lever
  (`chunking.rs:12-20`), moved within Fork A's scope as a safety-margin target.
  The parallelism conversation is paused.

## Open questions

- **Is `max_tokens` already truncating?** Fork B answers this: if passage text
  looks truncated mid-sentence or ends abruptly, the estimate is too lax. If
  passages are full and coherent, Threats 1-2 may be lower priority.
- **Does over-chunking hurt search quality?** The eval's passage-level grading
  catches this. False sentence splits = bad hit text. If passages read fine
  despite the naive splitter, Fork A scope can shrink.
- **When does the parallel path re-enter?** After the colleague demo, as developer
  play or when work-scale makes multi-hour ingest unacceptable. The channel-writer
  and embed/write seam split will be re-evaluated then.

## Session metadata

- Built in session 2026-06-16 with athenaeum-mcp at commit reflecting `lancedb 0.30`,
  `rmcp 1.7.0`, `nomic-embed-text` 768-dim.
- Two books successfully ingested and searched end-to-end before brief creation.
- Original brief (`.spar/brief.md`) reviewed; this supersedes it.
