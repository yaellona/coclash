# NixOS module: programs.teclash
#
# usage in configuration.nix:
#   imports = [ inputs.teclash.nixosModules.default ];
#   programs.teclash.enable = true;
#   # 可选：自定义 mihomo 路径（默认指向带权限的 wrapper）
#   # programs.teclash.mihomo_exe = lib.getExe pkgs.mihomo;
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.programs.teclash;

  wrapped = pkgs.runCommand "teclash-wrapped" {
    nativeBuildInputs = [ pkgs.makeWrapper ];
  } ''
    mkdir -p $out/bin
    cp -L ${cfg.package}/bin/teclash $out/bin/teclash
    wrapProgram $out/bin/teclash \
      --set-default TECLASH_MIHOMO_EXE "${cfg.mihomo_exe}"
  '';
in
{
  options.programs.teclash = {
    enable = lib.mkEnableOption "teclash TUI";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.teclash;
      description = "teclash package to install.";
    };

    mihomo_exe = lib.mkOption {
      type = lib.types.str;
      default = "${pkgs.mihomo}/bin/mihomo";
      description = ''
        mihomo executable injected via the TECLASH_MIHOMO_EXE environment
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
