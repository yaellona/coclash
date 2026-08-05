# NixOS module: programs.coclash
#
# usage in configuration.nix:
#   imports = [ inputs.coclash.nixosModules.default ];
#   programs.coclash.enable = true;
#   # 可选：自定义 mihomo 路径（默认指向带权限的 wrapper）
#   # programs.coclash.mihomo_exe = lib.getExe pkgs.mihomo;
{
  self,
}:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.programs.coclash;

  wrapped = pkgs.runCommand "coclash-wrapped" {
    nativeBuildInputs = [ pkgs.makeWrapper ];
  } ''
    mkdir -p $out/bin
    cp -L ${cfg.package}/bin/coclash $out/bin/coclash
    wrapProgram $out/bin/coclash \
      --set-default COCLASH_MIHOMO_EXE "${cfg.mihomo_exe}"
  '';
in
{
  options.programs.coclash = {
    enable = lib.mkEnableOption "coclash TUI";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.coclash;
      description = "coclash package to install.";
    };

    mihomo_exe = lib.mkOption {
      type = lib.types.str;
      default = "/run/wrappers/bin/mihomo";
      description = ''
        mihomo executable injected via the COCLASH_MIHOMO_EXE environment
        variable. Defaults to the security.wrappers wrapper which grants
        CAP_NET_ADMIN/CAP_NET_RAW/CAP_NET_BIND_SERVICE for TUN support.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ wrapped ];

    # 给 mihomo 授予 TUN 所需权限（无 setuid，进程以普通用户运行）
    security.wrappers.mihomo = {
      source = "${pkgs.mihomo}/bin/mihomo";
      capabilities = "cap_net_admin,cap_net_bind_service,cap_net_raw+eip";
      owner = "root";
      group = "users";
      permissions = "u+rx,g+rx";
    };
  };
}
