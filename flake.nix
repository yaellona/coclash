{
  description = "coclash - mihomo kernel TUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        rustPlatform = pkgs.rustPlatform;
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = "coclash";
          version = "0.1.0";
          src = lib.cleanSourceWith { src = ./.; };
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.makeWrapper
          ];
          buildInputs = [ pkgs.openssl ];

          postInstall = ''
            wrapProgram $out/bin/coclash \
              --prefix PATH : ${pkgs.mihomo}/bin
          '';

          meta = {
            description = "mihomo kernel TUI";
            homepage = "https://github.com/yaellona/coclash";
            license = lib.licenses.mit;
            mainProgram = "coclash";
          };
        };

        packages.coclash = self.packages.${system}.default;

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
        };
      }
    )
    // {
      nixosModules.default = import ./nixos/module.nix { inherit self; };
    };
}
