# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

每次请用中文回答我。

## 项目概述

这是一个基于 Tauri 2 + React 19 + Vite 7 的桌面应用程序项目，名为 wg-x。

## 技术栈

- **前端**: React 19.1.0 + Vite 7.0.4
- **桌面框架**: Tauri 2
- **后端**: Rust (Tauri 后端)
- **包管理器**: Yarn

## 开发命令

### 前端开发
```bash
# 启动 Vite 开发服务器（仅前端，不含 Tauri）
yarn dev

# 构建前端
yarn build

# 预览构建结果
yarn preview
```

### Tauri 开发
```bash
# 启动 Tauri 开发模式（包含热重载）
yarn tauri dev

# 构建 Tauri 应用
yarn tauri build

# 其他 Tauri 命令
yarn tauri [command]
```

### Rust 后端开发
```bash
# 在 src-tauri 目录下
cd src-tauri

# 检查 Rust 代码
cargo check

# 运行 Rust 测试
cargo test

# 格式化 Rust 代码
cargo fmt

# 运行 Clippy 检查
cargo clippy
```

## 项目架构

### 目录结构

```
.
├── src/                    # React 前端代码
│   ├── App.jsx            # 主应用组件
│   ├── main.jsx           # React 入口文件
│   └── App.css            # 样式文件
├── src-tauri/             # Tauri Rust 后端
│   ├── src/
│   │   ├── lib.rs         # 主库文件，包含 Tauri 命令和应用启动逻辑
│   │   └── main.rs        # 入口文件
│   ├── Cargo.toml         # Rust 依赖配置
│   ├── tauri.conf.json    # Tauri 配置文件
│   └── capabilities/      # Tauri 权限配置
├── index.html             # HTML 入口
├── vite.config.js         # Vite 配置
└── package.json           # Node.js 依赖配置
```

### 前后端通信

- 前端使用 `@tauri-apps/api/core` 的 `invoke` 函数调用 Rust 命令
- Rust 端使用 `#[tauri::command]` 宏定义可调用的命令
- 所有命令需在 `lib.rs` 的 `invoke_handler` 中注册

示例：
```javascript
// 前端调用
import { invoke } from "@tauri-apps/api/core";
const result = await invoke("greet", { name: "World" });
```

```rust
// Rust 端定义
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
```

### Tauri 配置

- `tauri.conf.json` 包含应用的元数据、窗口配置、构建配置等
- 开发服务器默认在 `http://localhost:1420`
- 构建输出目录为 `../dist`（相对于 src-tauri）

## 关键依赖

- `tauri-plugin-opener`: 用于在默认浏览器或应用中打开 URL
- `serde` & `serde_json`: Rust 序列化/反序列化
- React 19: 前端 UI 框架

## 添加新的 Tauri 命令

1. 在 `src-tauri/src/lib.rs` 中定义命令函数并加上 `#[tauri::command]`
2. 在 `invoke_handler` 中注册新命令：`tauri::generate_handler![greet, your_new_command]`
3. 在前端使用 `invoke("your_new_command", { args })` 调用
