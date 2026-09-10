{
  description = "Neddit: Private Reddit-compatible read service";

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

        neddit = package {
          binaryName = "neddit";
          cargoExtraArgs = "-p neddit";
          runtimeInputs = [ pkgs.yt-dlp pkgs.deno ];
        };
      in
      {
        checks = {
          inherit neddit;
        };

        packages = {
          default = neddit;
          inherit neddit;
        };
      }) // {
        nixosModules.default = import ./nix/module.nix { inherit self; };
        nixosModules.neddit = self.nixosModules.default;
      };
}
