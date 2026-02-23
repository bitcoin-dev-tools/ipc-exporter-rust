{
  description = "Bitcoin Core IPC metrics exporter (Rust, Cap'n Proto)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    bitcoin-node-src = {
      url = "github:ryanofsky/bitcoin/pr/ipc";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
      bitcoin-node-src,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;

      mkPkgs =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

      mkCraneLib =
        system:
        let
          pkgs = mkPkgs system;
          rust = pkgs.rust-bin.stable.latest.default;
        in
        (crane.mkLib pkgs).overrideToolchain rust;
    in
    {
      nixosModules.default = import ./module.nix;

      overlays.default = final: prev: {
        bitcoin-node = self.packages.${final.system}.bitcoin-node;
        bitcoind = self.packages.${final.system}.bitcoin-node;
        bitcoin-cli = self.packages.${final.system}.bitcoin-node;
      };

      packages = forAllSystems (
        system:
        let
          pkgs = mkPkgs system;
          craneLib = mkCraneLib system;
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type: (builtins.match ".*\\.capnp$" path != null) || (craneLib.filterCargoSources path type);
          };
          commonArgs = {
            inherit src;
            pname = "ipc-exporter-rust";
            version = "0.1.0";
            nativeBuildInputs = [ pkgs.capnproto ];
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        in
        {
          default = craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; });
          bitcoin-node = pkgs.bitcoind.overrideAttrs (old: {
            pname = "bitcoin-node";
            version = "30.99-ipc";
            src = bitcoin-node-src;
            preUnpack = "";
            doCheck = false;
            doInstallCheck = false;
          });
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = mkPkgs system;
          rust = pkgs.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
          };
        in
        {
          default = pkgs.mkShell {
            buildInputs = [
              rust
              pkgs.capnproto
              pkgs.just
            ];
          };
        }
      );
    };
}
