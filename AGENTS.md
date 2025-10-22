# 仓库指南

## 项目结构与模块组织
核心 React UI 位于 `src/`，按功能拆分为 `components/`（可复用 UI）、`pages/`（路由级视图，如 `TunnelManagementView`）、`hooks/`、`utils/`，以及集中在 `styles/` 的全局样式。静态资源放在 `public/`，构建产物输出到 `dist/`。桌面端特定代码位于 `src-tauri/`；`src-tauri/src/` 存放 Rust 服务（隧道生命周期、守护进程 IPC），`tauri.conf.json` 负责打包元数据。自动化脚本放在 `scripts/`，而用于 PR 文档的参考截图请放在 `screens/`。

## 构建、测试与开发命令
首次使用前运行 `npm install`，项目要求 Node 18+。使用 `npm run dev` 预览 Vite Web 页面，需要完整桌面壳与 WireGuard 守护进程绑定时使用 `npm run tauri dev`。`npm run build` 构建 Web 资源；通过 `npm run tauri build` 生成可交付的桌面应用。发布后运行 `npm run version:update` 同步桌面端与 Web 版本号。

## 编码风格与命名约定
React 文件遵循 2 空格缩进并使用双引号。React 组件使用 `PascalCase` 命名，Hook 和工具函数使用 `camelCase`。将有状态逻辑封装在 `hooks/` 下的自定义 Hook 中，网络或更新相关辅助函数放入 `utils/`。`src-tauri` 中的 Rust 代码应通过 `cargo fmt` 与 `cargo clippy`，并倾向按功能拆分模块（见 `tunnel.rs`、`daemon.rs`）。CSS 使用普通文件，新增样式与组件同目录放在 `styles/`，由入口组件引入。

## 测试指南
当前尚未接入自动化测试；为功能更新补充手动验证记录，涵盖隧道创建、守护进程生命周期和更新流程。如需添加单元测试，请使用 Vitest + React Testing Library，并将测试文件与组件同目录命名为 `*.test.jsx`，同时在 `package.json` 中加入 `test` 脚本。桌面端变更需通过 `npm run tauri dev` 验证；调试守护进程回归时请附上来自 `src-tauri` 的日志。

## 提交与 Pull Request 指南
提交信息遵循 Conventional Commits，可选 scope（如 `refactor(TunnelManagementView): …`）。摘要可使用英文或简体中文，但需保持祈使语气。Pull Request 应关联相关 issue，说明 UI 或守护进程影响，并在视觉变更时附上 `screens/` 中更新后的截图。记录手动测试步骤，并注明是否需要版本号提升或服务重启。
