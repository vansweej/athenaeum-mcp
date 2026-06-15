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
            # TODO: confirm pdfium derivation name — pdfium-binaries is the nixpkgs
            # attribute as of June 2026; verify with `nix search nixpkgs pdfium` if
            # the build fails to locate the dynamic library.
            pkgs.pdfium-binaries   # native shared library for pdfium-render
          ];

          env = {
            RUST_BACKTRACE = "1";

            # Tell pdfium-render where to find the dynamic library at runtime.
            PDFIUM_DYNAMIC_LIB_PATH =
              if pkgs.stdenv.isDarwin
              then "${pkgs.pdfium-binaries}/lib/libpdfium.dylib"
              else "${pkgs.pdfium-binaries}/lib/libpdfium.so";
          } // pkgs.lib.optionalAttrs pkgs.stdenv.isDarwin {
            # Extend the macOS dynamic linker search path so the pdfium shared
            # library is found at runtime without a full reinstall.
            DYLD_LIBRARY_PATH = "${pkgs.pdfium-binaries}/lib";
          };
        };
      }
    );
}
