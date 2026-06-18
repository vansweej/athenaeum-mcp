# Feature: Bound the Ollama embed HTTP call with configurable timeouts

**Context for the builder:** `athenaeum-core`'s `OllamaEmbedder` builds its `reqwest::Client`
with no timeout (`crates/core/src/embed.rs:53`), so a stalled Ollama (cold model load,
restart, half-open socket) makes every `search`/`upsert` hang indefinitely with no error
— the `CoreError::Http` path at `embed.rs:80` is never reached. This adds two `Duration`
fields to `Config`, backed by two module-level `pub const` defaults defined **once** in
`config.rs`, threads them into a new timeout-aware constructor, keeps the existing 3-arg
`new()` as a defaulting delegate (so no existing call site changes), and proves a stall
now yields `CoreError::Http` via a delayed wiremock test.

**Single source of truth:** the default durations are defined as `pub const DEFAULT_EMBED_TIMEOUT`
and `pub const DEFAULT_EMBED_CONNECT_TIMEOUT` in `config.rs`. Both `Config::default` and
`OllamaEmbedder::new` reference these consts — no literal duration is duplicated across
the crate.

**Build/verify gate (run after each phase, per AGENTS.md — all inside the nix shell):**
`nix develop --command cargo fmt`, then `nix develop --command cargo clippy -- -D warnings`,
then `nix develop --command cargo test -p athenaeum-core`. Import order is manual:
std → external → crate.

---

## Phase 1: Add timeout defaults and fields to Config

Commit message: `feat(core): add embed timeout consts and Config fields`

### Step 1: Define the default-duration consts and add the Config fields

In `crates/core/src/config.rs`:

- Add `use std::time::Duration;` to the imports (keep the existing `use std::path::PathBuf;`;
  both are std, group them together).
- After the imports and before the `Config` struct, define two module-level constants with
  `///` doc comments:
  - `pub const DEFAULT_EMBED_TIMEOUT: Duration = Duration::from_secs(60);` — doc: total
    deadline for the Ollama embed request; generous enough to tolerate a cold model load.
  - `pub const DEFAULT_EMBED_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);` — doc:
    TCP connect deadline; short because "cannot connect" is unambiguous.
  - These are `const` because `Duration::from_secs` is a stable `const fn`. Keep them `pub`
    (so callers constructing `Config` directly can reference the same defaults) but do **not**
    re-export them from `lib.rs` — they stay accessible as `athenaeum_core::config::DEFAULT_*`.
- Add two public fields to the `Config` struct, each with a `///` doc comment:
  `embed_timeout: Duration` (total request deadline) and
  `embed_connect_timeout: Duration` (TCP connect deadline).
- In `impl Default for Config`, set `embed_timeout: DEFAULT_EMBED_TIMEOUT` and
  `embed_connect_timeout: DEFAULT_EMBED_CONNECT_TIMEOUT` (reference the consts, not literals).
- `Config` already derives `Debug, Clone`; `Duration` supports both, so no derive change is needed.

### Step 2: Add config tests asserting the timeout defaults

In the `#[cfg(test)] mod tests` block at the bottom of `crates/core/src/config.rs`,
add two `#[test]` functions following the style of `default_embed_dim_is_768`:

- One asserting `Config::default().embed_timeout == std::time::Duration::from_secs(60)`.
- One asserting `Config::default().embed_connect_timeout == std::time::Duration::from_secs(5)`.

Assert against explicit `Duration::from_secs(..)` **literals** (not the consts) so the
test independently guards the documented default *values* — if someone changes a const,
the test must fail.

---

## Phase 2: Add a timeout-aware constructor to OllamaEmbedder

Commit message: `feat(core): build embed client with connect and request timeouts`

### Step 1: Add `with_timeouts` and make `new` delegate using the config consts

In `crates/core/src/embed.rs`, replace the body of the `impl OllamaEmbedder` block
(currently just `new` at lines 48–59):

- Add imports: `use std::time::Duration;` (std group, before the existing external imports
  `async_trait` / `serde`), and
  `use crate::config::{DEFAULT_EMBED_CONNECT_TIMEOUT, DEFAULT_EMBED_TIMEOUT};`
  (crate group, after the existing `use crate::error::CoreError;`).
- Add a constructor `pub fn with_timeouts(url: impl Into<String>, model: impl Into<String>,
  dim: usize, timeout: Duration, connect_timeout: Duration) -> Self`. It must build the
  client via `reqwest::Client::builder().connect_timeout(connect_timeout).timeout(timeout)
  .build().expect("failed to build reqwest client")` and construct `Self` with that client.
  Add a `///` doc comment noting that `build()` only fails on TLS backend initialization
  (an environment fault, not a runtime condition), which is why `expect` is acceptable.
- Change `new` to delegate:
  `pub fn new(url: impl Into<String>, model: impl Into<String>, dim: usize) -> Self`
  calls `Self::with_timeouts(url, model, dim, DEFAULT_EMBED_TIMEOUT,
  DEFAULT_EMBED_CONNECT_TIMEOUT)`. Keep its existing doc comment and add a sentence stating
  it uses the crate default timeouts (`DEFAULT_EMBED_TIMEOUT` / `DEFAULT_EMBED_CONNECT_TIMEOUT`),
  which also back `Config`. Do **not** change `new`'s signature — six existing test call sites
  and `Engine::new` depend on the 3-arg form.

Note: the `EmbedRequest`/`EmbedResponse` structs and the `impl Embedder for OllamaEmbedder`
block are unchanged. This introduces an intra-crate `embed → config` dependency; `config`
depends on nothing, so there is no import cycle.

---

## Phase 3: Thread the configured timeouts through Engine::new

Commit message: `feat(core): pass configured embed timeouts into OllamaEmbedder`

### Step 1: Use `with_timeouts` in Engine::new

In `crates/core/src/engine.rs`, in the `Engine<OllamaEmbedder>::new` method (around line 38–40,
inside the `#[cfg(not(tarpaulin_include))]` block), change the embedder construction from
`OllamaEmbedder::new(&config.ollama_url, &config.embed_model, config.embed_dim)` to
`OllamaEmbedder::with_timeouts(&config.ollama_url, &config.embed_model, config.embed_dim,
config.embed_timeout, config.embed_connect_timeout)`. No other lines in this method change;
`Store::open` and the struct construction stay as-is. No import change is needed
(`OllamaEmbedder` is already imported at the top of the file).

---

## Phase 4: Prove a stalled server yields CoreError::Http

Commit message: `test(core): assert embed times out into Http error on slow response`

### Step 1: Add a delayed-response wiremock timeout test

In `crates/core/src/embed.rs`, inside the existing `#[cfg(test)] mod tests` block
(alongside the other `ollama_embedder_*` wiremock tests near line 293), add
`#[tokio::test] async fn ollama_embedder_times_out_on_slow_response()`. It must:

- Open with `use std::time::Duration;` and
  `use wiremock::{matchers::{method, path}, Mock, MockServer, ResponseTemplate};`
  (same in-fn `use` style as `ollama_embedder_http_error`).
- Start a `MockServer`, and mount a `Mock` for `method("POST")` + `path("/api/embed")`
  that responds with a **valid** 4-dimension body
  `serde_json::json!({ "embeddings": [[0.1, 0.2, 0.3, 0.4]] })` wrapped in
  `ResponseTemplate::new(200).set_body_json(&response_body).set_delay(Duration::from_secs(2))`.
  The body must be valid so the resulting error provably comes from the deadline, not
  from parsing/validation.
- Construct the embedder with
  `OllamaEmbedder::with_timeouts(mock_server.uri(), "test-model", 4,
  Duration::from_millis(200), Duration::from_secs(5))` — request timeout (200ms) well
  under the 2s server delay.
- Call `embedder.embed(&["hello".to_string()]).await` and assert
  `matches!(result, Err(CoreError::Http(_)))`, with a failure message including `{result:?}`.

---

## Phase 5: Document the new configuration fields

Commit message: `docs: document embed timeout configuration fields`

### Step 1: Add the timeout fields to the README configuration table

In `README.md`, in the Configuration section table (the `| Field | Default | Description |`
table around lines 93–99), add two rows:
`embed_timeout` with default `60s` describing it as the total deadline for the Ollama
embed request (must tolerate a cold model load), and `embed_connect_timeout` with default
`5s` describing it as the TCP connect deadline (fails fast when Ollama is unreachable).
Keep the existing rows and table formatting intact. Do not modify the historical
`docs/plans/*` files, which intentionally record the prior constructor signature.
