{
  description = "athenaeum-mcp — local-first semantic-search MCP server";

  inputs = {
    nixpkgs.url     = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url     = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs     = import nixpkgs { inherit system overlays; };

        # Pin the toolchain to the channel declared in rust-toolchain.toml so
        # there is a single version source of truth.
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # nixpkgs' buildRustPackage, but using the pinned oxalica toolchain instead of
        # nixpkgs' default rustc/cargo.
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = "athenaeum-mcp-server";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [
            pkgs.protobuf
            pkgs.cmake
            pkgs.pkg-config
            pkgs.makeWrapper
          ];
          buildInputs = [
            pkgs.pdfium-binaries
          ];

          # buildRustPackage runs `cargo test` in the checkPhase, which exercises the
          # real pdfium path (ingest::ingest_pdf_end_to_end and
          # parser-spike::extracts_text_from_sample_pdf). pdfium-render's
          # Pdfium::default() resolves libpdfium only via the OS dynamic-linker search
          # path, so the checkPhase needs the same loader wiring the devShell gets from
          # its shellHook. One export covers every workspace test binary, since cargo
          # runs them in a single invocation. DYLD_LIBRARY_PATH on macOS,
          # LD_LIBRARY_PATH on Linux; append rather than clobber any existing value.
          preCheck =
            let libDir = "${pkgs.pdfium-binaries}/lib";
            in if pkgs.stdenv.isDarwin then ''
              export DYLD_LIBRARY_PATH="${libDir}''${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
            '' else ''
              export LD_LIBRARY_PATH="${libDir}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            '';

          # buildRustPackage installs every workspace binary. relevance-eval is a
          # hand-run, dev-shell-only evaluation instrument (live Ollama + human grader;
          # see crates/ingest/src/bin/relevance-eval.rs) — drop it rather than ship a
          # tool meant only for `nix develop`. The two deployed binaries both call
          # pdfium-render's Pdfium::default(), which resolves libpdfium only via the OS
          # loader path, so bake the pdfium-binaries lib dir onto that path with
          # wrapProgram (DYLD_LIBRARY_PATH on macOS, LD_LIBRARY_PATH on Linux).
          postInstall =
            let libVar = if pkgs.stdenv.isDarwin then "DYLD_LIBRARY_PATH" else "LD_LIBRARY_PATH";
            in ''
              rm -f $out/bin/relevance-eval
              for bin in athenaeum-mcp-server athenaeum-ingest; do
                wrapProgram $out/bin/$bin \
                  --prefix ${libVar} : ${pkgs.pdfium-binaries}/lib
              done
            '';
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            rustToolchain          # Rust compiler, cargo, rustfmt, clippy
            pkgs.cargo-tarpaulin   # >= 90% coverage enforcement (works on macOS)
            pkgs.pkg-config        # required by crates that link to system libs
            pkgs.protobuf          # protoc — required by the lance/arrow build used by lancedb
            pkgs.cmake             # required by the lancedb native build
            pkgs.git               # ensure git is available in the shell
          ];

          buildInputs = [
            pkgs.pdfium-binaries   # native shared library for pdfium-render
          ];

          env = {
            RUST_BACKTRACE = "1";
          };

          # pdfium-render's `Pdfium::default()` resolves libpdfium through the OS
          # dynamic-linker search path (it does NOT read any custom env var), so the
          # loader path must include the pdfium-binaries lib directory. Append rather
          # than clobber any value the caller already exported.
          shellHook =
            let libDir = "${pkgs.pdfium-binaries}/lib";
            in if pkgs.stdenv.isDarwin then ''
              export DYLD_LIBRARY_PATH="${libDir}''${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
            '' else ''
              export LD_LIBRARY_PATH="${libDir}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            '';
        };
      }
    );
}
