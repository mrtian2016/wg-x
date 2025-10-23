## WG-X (WireGuard X)

一个基于 Tauri 的跨平台 WireGuard 隧道管理与配置生成工具。支持 macOS / Linux / Windows，提供可视化的隧道启动/停止、服务端与 Peer 管理、历史记录与二维码导出、WebDAV 双向同步、以及一键生成多种场景配置（WireGuard、Surge、iKuai、MikroTik、OpenWrt）。内置自动更新与（Linux）守护进程模式。

> 运行环境：Node.js 18+、Rust/Cargo、Tauri 2。桌面端集成 Rust 后端；Web 预览可用 Vite 启动但不具备隧道控制能力。

### 截图

![](screens/iShot_2025-10-23_23.44.49.png)

### 功能特性

- 隧道管理
  - 列表查看、启动/停止、状态与流量统计（跨平台实现）
  - 支持服务端模式与客户端模式；智能生成接口名（各平台规则不同）
- 密钥与配置生成
  - 生成私钥/公钥、预共享密钥；由私钥计算公钥
  - 生成客户端配置预览与保存（含二维码）
  - 一键输出：WireGuard、Surge、iKuai、MikroTik、OpenWrt
- 服务端与历史
  - 多服务端配置管理（持久化至 App 数据目录）
  - 历史记录查看、筛选、导出 TXT/ZIP、清空
- WebDAV 同步
  - 手动/自动双向同步服务端配置与历史记录
  - 同步进度与最近同步时间展示
- 更新与日志
  - 自动更新（GitHub Releases 最新版本）
  - 应用/守护进程日志输出与保存
- Linux 守护进程（可选）
  - 通过 `wg-x daemon` 运行 root 模式守护进程
  - 提供 systemd 单元文件，支持安装/启停/开机自启


### 架构总览

- 前端：Vite + React（`src/`）
  - pages：`TunnelManagementView`、`ConfigGeneratorView`、`HistoryView`、`ServerManagementView`、`WebDavSettingsView`
  - components：Toast、ConfirmDialog、UpdateProgressDialog、DaemonPanel 等
  - utils：更新管理、通知等
- 后端：Tauri + Rust（`src-tauri/`）
  - 平台实现：`tunnel_macos.rs`、`tunnel_linux.rs`、`tunnel_windows.rs`
  - 核心隧道管理：`tunnel.rs`（启动/停止/状态、接口名规则、密钥转换、DNS 解析等）
  - 命令层：`src-tauri/src/commands/*`（密钥、模板生成、持久化、服务端、历史、WebDAV）
  - 同步模块：`webdav.rs`、`sync.rs`
  - Linux 守护：`daemon.rs`、`daemon_ipc.rs`、`daemon_install.rs`
- 配置与打包：`src-tauri/tauri.conf.json`（应用标识、外部二进制、自动更新端点等）


### 目录结构（节选）

```
.
├─ src/                        # React 前端
│  ├─ pages/
│  │  ├─ TunnelManagementView/
│  │  ├─ ConfigGeneratorView/
│  │  ├─ HistoryView/
│  │  ├─ ServerManagementView/
│  │  └─ WebDavSettingsView/
│  ├─ components/
│  ├─ hooks/ utils/ styles/
│  └─ App.jsx main.jsx
├─ src-tauri/                  # Rust + Tauri 后端
│  ├─ src/
│  │  ├─ tunnel.rs tunnel_*.rs
│  │  ├─ commands/*.rs
│  │  ├─ webdav.rs sync.rs
│  │  └─ daemon*.rs (Linux)
│  └─ tauri.conf.json
├─ public/ dist/               # 静态资源与构建产物
├─ screens/                    # 截图（用于 PR）
├─ scripts/                    # 自动化脚本（版本同步等）
├─ wg-x-daemon.service         # Linux systemd 单元文件
├─ package.json vite.config.js
└─ AGENTS.md CLAUDE.md
```


### 快速开始

- 安装依赖（首次）
  - Node.js 18+、Rust（含 `cargo`）
  - 平台依赖见“平台依赖与权限”
- 安装前端依赖

```bash
npm install
```

- Web 预览（仅 UI，无法控制隧道）

```bash
npm run dev
```

- 桌面开发（推荐，具备完整能力）

```bash
npm run tauri dev
```

- 构建 Web 资源 / 桌面安装包

```bash
npm run build
npm run tauri build
```

- 版本同步（发布后同步 Web 与桌面端版本号）

```bash
npm run version:update
```


### 平台依赖与权限

- macOS
  - 通过 `osascript` 进行一次性权限提升，启动 `wireguard-go`、配置 IP/路由并修改 UAPI socket 权限。
  - `wireguard-go` 作为外部二进制随应用打包（`bundle.externalBin`）。
- Linux
  - 仅需 `wireguard-go`（用户态实现），不依赖 `wg`/`wg-quick`。应用优先使用守护进程安装脚本放置的 `/opt/wg-x/wireguard-go`，否则从系统路径查找。
  - 支持守护进程模式：以 root 权限运行 `wg-x daemon`，监听本地 Unix Socket，负责隧道生命周期与统计。
- Windows
  - 依赖官方 WireGuard 客户端（`wg.exe`、`wireguard.exe`），程序会在 `PATH` 与常见安装路径中自动查找。
  - 隧道作为 Windows 服务/后台进程方式管理。

> 注意：不同平台的接口命名规则、权限模型与路由配置方式不同，应用已做适配，但仍需确保系统已安装必要依赖并允许网络接口创建。


### 使用指南（核心流程）

- 隧道列表（默认页）
  - 查看当前隧道、启动/停止、查看详情与每个 Peer 的完整客户端配置（支持复制/另存）
  - 服务端模式下支持“⚡ 快速添加客户端”（自动生成密钥与地址）与手动添加 Peer
- Peer 配置生成
  - 输入本地私钥/公钥、端口、DNS、AllowedIPs、Endpoint 等
  - 一键生成 WireGuard/Surge/iKuai/MikroTik/OpenWrt 配置；支持导出与二维码
- 服务器配置
  - 管理多个上游服务端配置（公钥、预共享密钥、AllowedIPs、Keepalive、接口名、Peer 递增 ID 等）
- 历史记录
  - 自动保存配置历史，支持筛选、详情查看、导出 TXT/ZIP、清空
- WebDAV 同步
  - 填写服务器地址、用户名/密码，支持启用自动同步与设置同步间隔
  - 页面与底部状态栏展示“最后同步时间”与同步结果


### Linux 守护进程

- 直接以守护进程模式运行

```bash
sudo wg-x daemon
```

- 使用随仓库提供的 systemd 单元文件（`wg-x-daemon.service`）

```bash
# 安装
sudo cp wg-x-daemon.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now wg-x-daemon

# 管理
sudo systemctl status wg-x-daemon
sudo systemctl restart wg-x-daemon
sudo systemctl stop wg-x-daemon
```

> 应用内（Linux）也提供了安装/卸载/启停守护进程的指令入口，详见 `src-tauri/src/daemon_install.rs` 与 UI 中的 Daemon 面板。


### 更新与日志

- 自动更新：Tauri Updater 使用 GitHub Releases 的 `latest.json` 作为端点（见 `tauri.conf.json`）。
- 查看版本：应用底部显示当前版本；命令行支持 `wg-x --version`。
- CLI 帮助：`wg-x --help` 显示可用参数与子命令（Linux 含 `daemon`）。
- 日志位置：标准输出、应用日志目录与 Webview（通过 `tauri-plugin-log`）。


### 手动验证建议（当前无自动化测试）

- 隧道管理
  - 创建服务端隧道，添加客户端 Peer，启动/停止并观察握手与流量统计
  - 客户端模式下，连接到已知可用服务端，验证连通性
- 配置生成
  - 分别导出 WireGuard/Surge/iKuai/MikroTik/OpenWrt 配置并验证可用性
- 历史记录与导出
  - 生成多条历史，按服务端筛选；导出 TXT 与 ZIP，检查内容
- WebDAV 同步
  - 开启自动同步、设置间隔；观察“最后同步时间”变化及拉取/推送结果
- 守护进程（Linux）
  - 安装 systemd 服务，重启系统后验证自动启动与隧道恢复

> 若你计划补充单元测试，请使用 Vitest + React Testing Library，并将测试文件命名为 `*.test.jsx` 且与组件同目录；同时在 `package.json` 增加 `test` 脚本。


### 构建与发布要点

- 桌面构建：`npm run tauri build` 会将前端产物打包到 Tauri 安装包。
- 外部二进制：`wireguard-go` 通过 `bundle.externalBin` 随应用分发（macOS）；Linux/Windows 依赖系统已安装工具。
- 版本同步：发布后执行 `npm run version:update` 保持前后端版本一致。





### 贡献与提交

- 提交信息：遵循 Conventional Commits，可选 scope（如 `refactor(TunnelManagementView): …`）。
- PR 要求：关联 issue，说明影响范围（UI/守护进程），视觉变更附 `screens/` 截图；记录手动测试步骤，并注明是否需要版本提升或服务重启。


### 许可证

本仓库未显式声明许可证。如需在其他项目中使用或分发，请先与作者确认。
