# NixOS module: programs.coclash
#
# usage in configuration.nix:
#   imports = [ inputs.coclash.nixosModules.default ];
#   programs.coclash.enable = true;
#
# 注意：mihomo 已内嵌在 coclash 二进制中（Linux 运行时从内存执行，不落盘），
# 不需要安装 nixpkgs 的 mihomo。TUN 默认开箱可用：enable 时通过
# security.wrappers.coclash 给 coclash 授予 CAP_NET_ADMIN/CAP_NET_RAW，
# wrapper 会把 capabilities 提升进 ambient set 后执行 coclash，
# 内嵌 mihomo 随之继承，无需手动 setcap。
# 不想要 capabilities 时设置 programs.coclash.tun = false（需自行处理内核权限）。
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
in
{
  options.programs.coclash = {
    enable = lib.mkEnableOption "coclash TUI";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.coclash;
      description = "coclash package to install.";
    };

    tun = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Whether to grant coclash CAP_NET_ADMIN/CAP_NET_RAW via a setcap
        security wrapper, so that the embedded mihomo can use TUN without
        any manual `setcap`. The wrapper raises the capabilities into the
        ambient set, which coclash passes on to mihomo.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    security.wrappers.coclash = lib.mkIf cfg.tun {
      source = lib.getExe cfg.package;
      owner = "root";
      group = "root";
      capabilities = "cap_net_admin,cap_net_raw+ep";
    };
  };
}
