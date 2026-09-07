{
  description = "Neddit: Private Reddit-compatible read API proxy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, crane, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        inherit (pkgs) lib;

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "x86_64-unknown-linux-musl" ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;


        src = lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = craneLib.filterCargoSources;
        };

        package = cargoExtraArgs: craneLib.buildPackage {
          inherit src;
          strictDeps = true;
          doCheck = false;

          inherit cargoExtraArgs;
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        };

        nedditApi = package "-p neddit-api";
        nedditWeb = package "-p neddit-web";
      in
      {
        checks = {
          neddit-api = nedditApi;
          neddit-web = nedditWeb;
        };

        packages = {
          default = nedditApi;
          neddit-api = nedditApi;
          neddit-web = nedditWeb;
        };
      }) // {
        nixosModules.default = import ./nix/module.nix { inherit self; };
        nixosModules.neddit = self.nixosModules.default;
      };
}
