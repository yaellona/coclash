## 介绍

这是基于mihomo内核的tui。

支持系统代理与tun模式。

> 做这玩意的契机是，我主用的系统的nixos，不知道为什么会clash verge rev有时候会抽风，导入不了url，于是我打算自己玩mihomo内核。
>
> 但是发现自己解决不了linux的系统代理热切换。只能修改linux的系统代理，然后mihomo开了不关QWQ.

## 安装

`coclash` **自带 mihomo**（构建期压缩嵌入），无需预装任何东西：

- `coclash`：TUI；Linux 默认把内核解压到内存（memfd）直接执行，运行期不落盘；Windows 释放为退出即删的临时文件。
- `coclash core [参数]`：等价原生 mihomo，参数原样透传（`-d`/`-f`/`-v`…），Unix 下直接替换进程（PID/信号/退出码一致）。

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

`enable` 会安装 coclash（mihomo 内嵌在二进制里，不依赖 nixpkgs 的 mihomo），并默认通过 `security.wrappers` 给 coclash 授予 `CAP_NET_ADMIN/CAP_NET_RAW`——**TUN 开箱可用，无需手动 setcap**。不想要 capabilities 时设置 `programs.coclash.tun = false`。

原理：`security.wrappers` 生成 `/run/wrappers/bin/coclash`（在 shell PATH 中优先），它把 capabilities 提升进 ambient set 后再执行 coclash，内嵌 mihomo 随之继承。请从 PATH 运行 `coclash`；直接执行 store 路径不会带权限。

### archlinux以及其他发行版

`todo`📒✍️

## 用法

首次进入`coclash`的时候，`mihomo`启动了≠能用了，如果发现读取`mihomo`端口失败了，说明`mihomo`还没有下载`GeoSite`数据库，需要等待一段时间下载数据库。

不想要 TUI 时可直接跑内核（参数与原生 mihomo 完全一致）：

```bash
coclash core -v
coclash core -d ~/.config/coclash -f config.yaml
```

### 内嵌 mihomo

- 版本固定在 `mihomo.lock`，构建时自动取用（优先 `vendor/mihomo/<target>/mihomo[.exe]`，没有才下载并解压到该目录；`COCLASH_EMBED_MIHOMO` 可指定构建期来源，供 Nix/离线构建使用）。
- 运行时来源（`coclash core` 与 TUI 启动共用）：
  - Linux：解压到内存（`memfd_create`）直接执行，进程显示为 `memfd:mihomo`，运行期不落盘；
  - Windows：释放到 `%TEMP%\coclash-<pid>\mihomo.exe`，内核退出后自动删除；coclash 先退出时残留目录由下次启动清理；
  - 兜底（内核不支持内存执行/临时文件失败）或显式要求时释放到 `{cache_dir}/coclash/bin/<sha8>/mihomo[.exe]`（Windows 为 `%LOCALAPPDATA%`）；`COCLASH_MIHOMO_DIR` 指定缓存根、`COCLASH_MIHOMO_EXTRACT=1`，两者都会强制落盘。
- 更新 mihomo：改 `mihomo.lock` 的版本/资产/哈希后重新编译（`vendor/mihomo/` 下对应文件需删除）。
- 不需要内嵌时可用 `cargo build --no-default-features`（此时无法启动 mihomo）。
- 内嵌的 mihomo 以 GPL-3.0 分发，许可证见 [assets/licenses/mihomo-GPL-3.0.txt](./assets/licenses/mihomo-GPL-3.0.txt)，源码：<https://github.com/MetaCubeX/mihomo>。

### Linux TUN 权限

mihomo 默认在内存中执行，没有磁盘文件可以 `setcap`，权限需要给 **coclash 本体**：

- **NixOS**：`programs.coclash.enable = true` 已自动完成（见上），TUN 直接可用；不要手动 `setcap /run/wrappers/bin/coclash`，那会覆盖 wrapper 依赖的 `cap_setpcap`，破坏 ambient 传递。
- **其他发行版**：给 coclash 授权后由它启动内核（启动时以 ambient capability 传给 mihomo）：

```bash
sudo setcap cap_net_admin,cap_net_raw+eip $(which coclash)
```

（若使用强制落盘模式，也可以继续对释放出的 mihomo 单独 `setcap`。）

### Geo 数据源

GeoIP/GeoSite 默认从国内可达的 jsDelivr 镜像（`testingcf.jsdelivr.net`）下载，可在 `{config_dir}/coclash/config.yaml` 的 `geox-url` 字段自行更换（`geoip` / `geosite` / `mmdb` 三个键）。

### 进程管理

- TUI 关闭时**不会**杀掉 mihomo 进程（进程与 TUI 解耦）。
- 运行状态**只看控制端口是否可达**，不区分实例由谁启动；按 `s` 直接启停：
  端口可达 → 停止（按控制端口找到属主 mihomo 进程结束），否则 → 启动内嵌 mihomo。
- 停止按「控制端口属主」定位，不会误杀监听其它端口的 mihomo；若端口被非 mihomo 程序占用，
  停止会报「未找到监听控制端口的 mihomo 进程」，启动则报端口占用。
- 启动失败但进程残留时（端口未就绪），仍可按 `s` 停止。
- `coclash core` 启动的实例同样能被 TUI 停止（按 `/proc/<pid>/exe` 识别内存执行的 `memfd:mihomo`）。
- mihomo 进程的 stdout/stderr 会写入 `{config_dir}/coclash/mihomo.log`，按 `l` 可在 TUI 内查看。

### 心跳同步

TUI 常驻一个心跳任务（快路径默认 3 秒一次，`settings.json` 的 `heartbeat_interval_ms` 可改，0 = 关闭）：

- **快路径（每 tick）**：端口探测、系统代理状态、累计流量（`/connections`）。
- **全量（每 10 tick ≈ 30s）**：额外同步内核版本（`/version`）、模式/端口/TUN/DNS（`/configs`）、
  节点列表与当前节点（`/proxies/{group}`）；按 `r` 可立即触发一次全量，无需等待。
- 端口可达/不可达的跃迁会更新运行状态（不扫描进程表，不做实例归属判断）。
- API 连续失败时按指数退避重试（最多 8 倍周期）；就绪/断开日志各只记一次。
- 状态面板优先显示运行时信息，停止时回落到本地 `config.yaml`；设置页仍只编辑本地配置，
  与运行中不一致时以灰色 `（运行时: X）` 提示。
- 节点列表刷新按名字保留测速结果。

### mihomo API

TUI 通过 mihomo 的 external-controller RESTful API 交互（心跳同步、测速、切换节点/订阅、重载配置），接口与错误语义见 [mihomo-api.md](./mihomo-api.md)。

## 代码结构

```
src/
├── main.rs        入口：CLI 分派（core 直通内核 / 默认 TUI）+ 事件循环
├── cli.rs         `coclash [core|help]` 参数解析
├── core/          与 mihomo 的纯 IO（RESTful API、进程、config.yaml、系统代理）
│   └── mihomo/    内嵌内核（embedded）与进程/启动平台层（process/{unix,windows}.rs）
├── manager/       共享状态与命令层
│   ├── state.rs       AppState { logs, config, mihomo }
│   ├── commands.rs    副作用契约 Effect/ConfigChange 与 Manager::exec
│   ├── tasks.rs       用户触发的异步任务
│   └── heartbeat.rs   心跳：唯一的状态同步实现
└── tui/           绘制与按键
    ├── cmd.rs         页面动作 Cmd（导航 Nav + 副作用 Effect）
    ├── action.rs      按键绑定（快捷键 + 执行函数 + 描述）
    ├── page.rs        页面接口 Page（只读 AppState，返回 Cmd）
    └── pages/         各页面 + 手写注册表（无过程宏）
```

新增页面：在 `tui/pages/` 新建文件实现 `Page`（`BINDINGS` + `draw`），
再在 `pages/mod.rs` 登记 `PageId` 变体与 `slots.insert(...)` 即可；
按键、帮助与底部栏文案全部由 `BINDINGS` 自动生成。

## 界面展示

1. windows中

![tui展示](./assets/windows_image.png)

2. linux中

![tui展示](./assets/linux_image.png)

## TODO

1. ~~添加tun模式。~~
2. ~~提供mihomo自动安装方案。~~（构建期内嵌 + 运行时释放，见「内嵌 mihomo」）
3. ~~打nix包。~~
4. ~~静默启动。~~
5. ~~mihomo的进程和tui解耦，关闭tui不关闭mihomo~~
6. ~~提供直连、规则、端口等修改。~~（设置窗口 `e`：模式/端口/规则编辑）
