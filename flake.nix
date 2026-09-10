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

        package = { binaryName, cargoExtraArgs, runtimeInputs ? [ ] }: craneLib.buildPackage {
          inherit src;
          strictDeps = true;
          doCheck = false;

          nativeBuildInputs = lib.optionals (runtimeInputs != [ ]) [ pkgs.makeWrapper ];
          postInstall = lib.optionalString (runtimeInputs != [ ]) ''
            wrapProgram $out/bin/${binaryName} --prefix PATH : ${lib.makeBinPath runtimeInputs}
          '';

          inherit cargoExtraArgs;
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
        };

        nedditApi = package {
          binaryName = "neddit-api";
          cargoExtraArgs = "-p neddit-api";
          runtimeInputs = [ pkgs.yt-dlp pkgs.deno ];
        };
        nedditWeb = package {
          binaryName = "neddit-web";
          cargoExtraArgs = "-p neddit-web";
          runtimeInputs = [ pkgs.yt-dlp pkgs.deno ];
        };
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
