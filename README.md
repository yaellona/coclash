## 介绍

这是基于mihomo内核的tui。

支持系统代理与tun模式。

> 做这玩意的契机是，我主用的系统的nixos，不知道为什么会clash verge rev有时候会抽风，导入不了url，于是我打算自己玩mihomo内核。
>
> 但是发现自己解决不了linux的系统代理热切换。只能修改linux的系统代理，然后mihomo开了不关QWQ.

## 安装

### windows

先留个`todo`暂时没做安装脚本喵。

### nixos

```nix
# flake 输入
inputs.coclash.url = "github:yaellona/coclash";

# configuration.nix
imports = [ inputs.coclash.nixosModules.default ];
programs.coclash.enable = true;
```

`enable` 后默认会给mihomo提供sudo权限，不需要再`sudo`给`coclash`了。

### archlinux以及其他发行版

`todo`📒✍️

## 用法

首次进入`coclash`的时候，`mihomo`启动了≠能用了，如果发现读取`mihomo`端口失败了，说明`mihomo`还没有下载`GeoSite`数据库，需要等待一段时间下载数据库。

### Geo 数据源

GeoIP/GeoSite 默认从国内可达的 jsDelivr 镜像（`testingcf.jsdelivr.net`）下载，可在 `{config_dir}/coclash/config.yaml` 的 `geox-url` 字段自行更换（`geoip` / `geosite` / `mmdb` 三个键）。

### 进程管理

- TUI 关闭时**不会**杀掉 mihomo 进程（进程与 TUI 解耦）。
- mihomo 运行状态不依赖任何文件记录，而是**直接扫描系统进程表**：找到命令行含 `{config_dir}/coclash` 的 mihomo 进程即视为运行中；按 `s` 停止时也只停止匹配该 config_dir 的实例，外部启动（命令行不含本 config_dir）的不会被误杀，需自行关闭。
- 启动失败但进程残留时（端口未就绪），仍可按 `s` 停止。
- mihomo 进程的 stdout/stderr 会写入 `{config_dir}/coclash/mihomo.log`，按 `l` 可在 TUI 内查看。

### mihomo API

TUI 通过 mihomo 的 external-controller RESTful API 交互（拉取节点、测速、切换节点/订阅、重载配置），接口与错误语义见 [mihomo-api.md](./mihomo-api.md)。

## 界面展示

1. windows中

![tui展示](./assets/windows_image.png)

2. linux中

![tui展示](./assets/linux_image.png)

## TODO

1. ~~添加tun模式。~~
2. 提供mihomo自动安装方案。(至少nixos实现了不是吗🤣)
3. ~~打nix包。~~
4. ~~静默启动。~~
5. ~~mihomo的进程和tui解耦，关闭tui不关闭mihomo~~
6. ~~提供直连、规则、端口等修改。~~（设置窗口 `e`：模式/端口/规则编辑）
