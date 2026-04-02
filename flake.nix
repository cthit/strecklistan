{
  description = "Strecklistan - A simple web-shop";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
            "rustfmt"
          ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        # Dependencies for the backend (Diesel/Postgres + OpenSSL)
        backendDeps = with pkgs; [
          openssl
          postgresql_17
          pkg-config
        ];

        # Dependencies for the frontend (Trunk/Wasm)
        frontendDeps = with pkgs; [
          trunk
          wasm-pack
          wasm-bindgen-cli
        ];

        # General development tools
        devTools = with pkgs; [
          rustToolchain
          cargo-make
          diesel-cli
          bacon
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          name = "strecklistan-dev";

          nativeBuildInputs = devTools ++ backendDeps ++ frontendDeps;

          shellHook = ''
            export DATABASE_URL="postgres://postgres:password@localhost:5432/strecklistan"
          '';
        };
      }
    );
}
