# NixOS module: programs.coclash
#
# usage in configuration.nix:
#   imports = [ inputs.coclash.nixosModules.default ];
#   programs.coclash.enable = true;
#
# 注意：mihomo 已内嵌在 coclash 二进制中（构建期压缩嵌入，运行时释放到缓存目录），
# 不需要安装 nixpkgs 的 mihomo，也不再需要 security.wrappers。
# Linux TUN 需要给释放出的 mihomo 手动授予 capabilities（coclash 日志里会给出具体路径）：
#   sudo setcap cap_net_admin,cap_net_raw+eip ~/.cache/coclash/bin/<sha>/mihomo
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
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
