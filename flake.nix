{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { self, nixpkgs, flake-utils, fenix }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        cargo = fromTOML (builtins.readFile ./Cargo.toml);
        version = cargo.workspace.package.version;
        makePkg = path: pkgs.callPackage path {
                  lib = pkgs.lib;
                  inherit version;
                };

        buildToolchain = fenix.packages.${system}.stable.withComponents [
          "cargo"
          "rustc"
          "rust-src"
          "clippy"
        ];
        toolchain = fenix.packages.${system}.combine [
          buildToolchain
          fenix.packages.${system}.latest.rustfmt
        ];

      in
        {
          packages = {
            server = makePkg ./server/nix/package.nix;
            client = makePkg ./client/nix/package.nix;
          };
          devShells.default = pkgs.mkShell {
            buildInputs = with pkgs; [
              toolchain
              pkg-config
              openssl
              stdenv.cc.cc.lib
            ];

            shellHook = ''
              export RUST_SRC_PATH="${toolchain}/lib/rustlib/src/rust/library"
              cargo --version
              echo "Прошу Вас, сделайте мне красиво!"
            '';
          };
        }
    ) // {
      nixosModules = {
        server = import ./server/nix/module.nix self;
        client = import ./client/nix/module.nix self;
        default = { ... }: {
          imports = [ self.nixosModules.server self.nixosModules.client ];
        };
      };
    };
}
