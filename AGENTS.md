# AGENTS.md — athenaeum-mcp

Local-first semantic-search MCP server (Rust workspace, 4 crates). Full docs live
in `README.md` and `docs/` (architecture, setup, ingestion, integration). This file
only captures the non-obvious facts an agent would otherwise get wrong.

## Build & verify — always inside the Nix shell

- Every cargo command must run in the dev shell: `nix develop --command cargo <args>`.
  The flake supplies pdfium, protobuf, and cmake that Cargo cannot; plain `cargo`
  fails to build/link.
- **No CI exists — you are the verification gate.** Before declaring work done, run:
  1. `nix develop --command cargo fmt`
  2. `nix develop --command cargo clippy -- -D warnings`  (zero warnings enforced)
  3. `nix develop --command cargo test`
  4. `nix develop --command cargo tarpaulin`  (coverage target ≥ 90%)
- Single crate: `... cargo test -p athenaeum-core`.
  Single test: `... cargo test -p athenaeum-core engine::tests::add_and_search_end_to_end`.
- Toolchain is pinned to **stable 1.96** (`rust-toolchain.toml`).

## pdfium / nix gotchas

- `Pdfium::default()` finds `libpdfium` only via the OS loader path (`LD_LIBRARY_PATH`
  on Linux, `DYLD_LIBRARY_PATH` on macOS); there is no `PDFIUM_DYNAMIC_LIB_PATH` lookup.
  Two test paths need it: `parser-spike::extracts_text_from_sample_pdf` and the ingest
  PDF path (`ingest::ingest_pdf_end_to_end`). The loader path is wired in two places:
  the devShell `shellHook` (interactive `cargo test`) and the package's `preCheck` (the
  `nix build` checkPhase). One `preCheck` export covers both crates, since cargo runs
  all workspace test binaries in a single invocation. `pdfium-binaries` ships an
  unversioned `lib/libpdfium.so` / `libpdfium.dylib` that pdfium-render `dlopen`s by
  leaf name, so `LD_LIBRARY_PATH` resolves it directly (SONAME is irrelevant); only if a
  Linux `nix build` somehow fails to locate it is an rpath/patchelf step needed.
- After editing `flake.nix`, re-enter the shell (`nix develop`) — a running shell won't
  pick up hook changes.

## reqwest / CA-cert nix gotcha

- The six `core` `embed::tests::ollama_embedder_*` tests build a `reqwest::Client`
  (`crates/core/src/embed.rs`, `OllamaEmbedder::with_timeouts` — the
  `.expect("failed to build reqwest client")`). reqwest 0.13's `rustls` feature pulls
  `rustls-platform-verifier`, which loads the OS trust store **eagerly at client-build
  time**. The hermetic `nix build` checkPhase has no trust store, so it panics with
  `General("No CA certificates were loaded from the system")` — even though those tests
  use wiremock over plain HTTP and never do a TLS handshake.
- **Active fix (A1):** `preCheck` exports
  `SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt`, and `pkgs.cacert` is in
  `nativeBuildInputs`. On Linux the verifier delegates to `rustls-native-certs`, which
  loads **only** from `SSL_CERT_FILE` when set (proven for the locked
  `rustls-platform-verifier 0.7.0`). Don't remove this export — the build is hermetic
  and won't otherwise have certs. This is the same checkPhase that wires the pdfium
  loader path above; both are sandbox needs the host supplies for free.
- **Darwin caveat / fallback (A2):** on macOS the verifier uses the Keychain
  (`security-framework`) and may ignore `SSL_CERT_FILE`; A1 passes the M1/M5 `nix build`
  only because the Darwin sandbox still exposes system roots. If a Darwin `nix build`
  ever fails on certs, switch to the fully-hermetic A2 (carries roots in the binary,
  reads no OS store on any platform):
  1. `Cargo.toml`: change reqwest to `features = ["json", "rustls-no-provider"]` (drops
     the platform verifier) and add `rustls` + `webpki-roots = "1"`; add both to
     `crates/core/Cargo.toml` `[dependencies]` too.
  2. `crates/core/src/embed.rs`: build a `rustls::ClientConfig` from
     `webpki_roots::TLS_SERVER_ROOTS` and pass it via
     `reqwest::ClientBuilder::use_preconfigured_tls(...)`.
  3. `flake.nix`: revert the A1 wiring — drop `pkgs.cacert` from `nativeBuildInputs` and
     remove the `SSL_CERT_FILE` export so `preCheck` returns to pdfium-only:
     ```nix
     nativeBuildInputs = [
       pkgs.protobuf
       pkgs.cmake
       pkgs.pkg-config
       pkgs.makeWrapper
     ];

     preCheck =
       let libDir = "${pkgs.pdfium-binaries}/lib";
       in if pkgs.stdenv.isDarwin then ''
         export DYLD_LIBRARY_PATH="${libDir}''${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
       '' else ''
         export LD_LIBRARY_PATH="${libDir}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
       '';
     ```
  Don't run A1 and A2 together — A2 replaces A1.

## rustfmt quirk

- `rustfmt.toml` sets `imports_granularity = "Crate"` and `group_imports = "StdExternalCrate"`,
  which are **nightly-only**. On the pinned stable toolchain they are silently ignored
  (cargo fmt prints a warning — don't try to "fix" it). Order imports manually:
  std → external → crate, merged per-crate, to match existing files.

## Testing conventions

- Tests use `FakeEmbedder` (deterministic) + `tempfile::tempdir()` for the store —
  never hit a real Ollama.
- The only live-Ollama test is `#[ignore]`d (`ollama_embedder_returns_768_dim_vectors`);
  run via `cargo test -- --ignored` only when Ollama + `nomic-embed-text` are up.
- pdfium/epub tests rely on committed binary fixtures in `crates/parser-spike/tests/fixtures/`.
  The ingest EPUB test reads the sibling fixture by relative path
  (`../parser-spike/tests/fixtures/sample.epub`); don't move or rename these.

## Crates

- `core` — Engine / Embedder / Store / Config; the search engine.
- `ingest` — extract + chunk, plus the `athenaeum-ingest` CLI binary (bulk loader).
- `mcp-server` — the `rmcp` MCP binary (the spine); `search` + `ingest_file` tools.
- `parser-spike` — **permanent** pdfium/epub version canary, deliberately NOT part of the
  pipeline. Don't delete it or wire it into ingest.

## Repo-specific gotchas

- No env-var config. Defaults are compile-time in `crates/core/src/config.rs`; override by
  constructing `Config` directly (tests pass a tempdir `db_path`).
- Default `db_path` is the **relative** `./data/athenaeum` — run the CLI and server from the
  repo root or they touch different stores.
- LanceDB `Store::add()` is append-only with no dedup; re-ingesting a file duplicates rows
  (see `docs/ingestion.md`).
- Bumping `lancedb` requires bumping `arrow-array` / `arrow-schema` in lockstep to the arrow
  version it re-exports (currently 58 for lancedb 0.30).

## Don't commit the corpus

- `.gitignore` blocks `data/`, `corpus/`, and `*.pdf` / `*.epub` / `*.mobi`. The only
  intentional exception is the canary fixtures under `crates/parser-spike/tests/fixtures/`.
  Never add real books or papers.

## Commits

- Conventional Commits, scoped: `feat(ingest):`, `fix(core):`, `chore:`, `docs:`.
