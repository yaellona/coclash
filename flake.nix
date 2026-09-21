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

        # 构建期内嵌的 mihomo 资产（版本与哈希需与 mihomo.lock 保持一致）。
        # Nix 沙箱没有网络，先由 fetchurl 取回 store 路径，
        # 再通过 COCLASH_EMBED_MIHOMO 注入 build.rs。
        mihomoAssets = {
          x86_64-linux = {
            url = "https://github.com/MetaCubeX/mihomo/releases/download/v1.19.31/mihomo-linux-amd64-v1.19.31.gz";
            hash = "sha256-1edLvdvf/0mhrvd3W/WRHaWfDXGW7VCaCskUs2U91fE=";
          };
          aarch64-linux = {
            url = "https://github.com/MetaCubeX/mihomo/releases/download/v1.19.31/mihomo-linux-arm64-v1.19.31.gz";
            hash = "sha256-ng8Rr784QmuL2I/cWUZ4+BYcV+zLTht3rLErSTkE8dQ=";
          };
        };
        mihomoAsset = mihomoAssets.${system} or null;
        mihomoSrc = if mihomoAsset == null then null else pkgs.fetchurl mihomoAsset;
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = "coclash";
          version = "0.1.0";
          src = lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              let
                base = baseNameOf path;
              in
              !(base == "vendor" || base == "target" || base == "result" || base == ".git");
          };
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];

          preBuild = lib.optionalString (mihomoSrc != null) ''
            export COCLASH_EMBED_MIHOMO=${mihomoSrc}
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
