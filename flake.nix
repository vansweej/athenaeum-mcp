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
      in
      {
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
