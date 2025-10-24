# Release 0.2.4

**发布日期**: 2025-10-24

## 🎉 主要更新

### 🆕 新功能

#### 1. **WireGuard 配置导入功能**
- ✨ 支持一键导入 `.conf` 格式的 WireGuard 配置文件
- 🤖 自动解析配置文件内容（Interface 和 Peer 部分）
- 🔍 智能检测运行模式：
  - 单个 Peer → 客户端模式
  - 多个 Peer → 服务端模式
- 📋 自动填充配置字段：
  - PrivateKey (私钥)
  - Address (本地地址)
  - DNS (DNS 服务器)
  - MTU (最大传输单元)
  - Peer 配置 (PublicKey, Endpoint, AllowedIPs, PresharedKey 等)
- 🔑 自动计算并显示公钥

#### 2. **二维码图像导入功能** ⭐ 新增
- 📸 支持从二维码图像文件导入 WireGuard 配置
- 🖼️ 支持的图像格式：PNG, JPG/JPEG, GIF, BMP, WebP
- 🔧 自动识别二维码内容并解析配置
- ⚙️ 完整的错误处理和用户提示
- 📲 与文件导入共用一套配置解析逻辑，确保一致性

### 🎨 UI/UX 改进

- **导入按钮重设计**：
  - 新增两个并排的导入按钮（各占 50% 宽度）
  - `📥 文件导入` - 从配置文件导入
  - `📱 二维码导入` - 从二维码图像导入
  - 仅在客户端模式且新建配置时显示

- **模式选择器优化**：
  - 清理了不必要的导入选项
  - 简化了选择流程，聚焦于模式选择

### 🔧 技术改进

#### 后端 (Rust/Tauri)
- 新增 `read_file_as_base64` 命令用于二进制文件读取
- 支持返回 Base64 编码的文件内容，便于前端处理二进制数据
- 完整的错误处理和提示

#### 前端 (React)
- 新增 `jsqr@1.4.0` 依赖用于二维码识别
- 实现 `parseWireGuardConfig()` 函数（~90 行）：
  - 支持 INI 格式配置文件解析
  - 正确处理多个 Peer 配置
  - 自动跳过注释行和空行
  - 支持两种 PSK 格式（PresharedKey 和 PreSharedKey）

- 实现 `handleImportFromQrcodeImage()` 函数（~120 行）：
  - 文件选择对话框与图像格式筛选
  - Canvas API 获取图像数据
  - jsQR 库识别二维码
  - 完整的配置解析和验证流程
  - 自动打开配置表单

### 📝 文档更新 (v0.2.3 → v0.2.4)

重构了项目 README，包含：
- 🌟 详细的功能特性列表
- 📸 应用界面预览
- 🛠️ 技术栈说明
- 📦 安装指南和系统要求
- ⚠️ macOS 用户注意事项（解决未签名应用的问题）
- 🚀 快速开始指南
- 📂 项目架构和目录结构说明
- 🔗 相关资源链接

---

## 📊 提交统计

| 类型 | 提交数 | 说明 |
|------|--------|------|
| feat | 3 | 新功能实现 |
| docs | 1 | 文档更新 |
| chore | 1 | 版本更新 |
| **总计** | **5** | |

---

## 📝 提交详情

### feat(tauri): 添加文件读取功能并支持导入 WireGuard 配置
- 新增 Tauri 命令 `read_file_content` 用于读取文件内容
- 前端实现配置文件导入和解析逻辑
- 添加 dialog 权限声明
- 支持自动检测运行模式

**文件变更**:
- `src-tauri/src/commands/misc_commands.rs` (+6)
- `src-tauri/src/lib.rs` (+1)
- `src-tauri/capabilities/default.json` (+1)
- `src/pages/TunnelManagementView/index.jsx` (+161)
- `src/pages/TunnelManagementView/components/ModeSelector.jsx` (+22/-4)

### feat(TunnelManagementView): 调整模式选择器并优化配置导入逻辑
- 将导入按钮位置从模式选择器移至配置表单内
- 限制导入功能仅在适当的上下文显示
- 优化用户导入流程

**文件变更**:
- `src/pages/TunnelManagementView/index.jsx` (+31/-24)
- `src/pages/TunnelManagementView/components/ModeSelector.jsx` (+22/-44)

### feat(tunnel): 新增从二维码图像导入配置功能
- 新增 `read_file_as_base64` 后端命令用于二进制文件读取
- 集成 `jsqr@1.4.0` 库进行二维码识别
- 实现完整的二维码导入流程
- 支持 PNG, JPG, GIF, BMP, WebP 等多种图像格式

**文件变更**:
- `package.json` (+1)
- `src-tauri/src/commands/misc_commands.rs` (+7)
- `src-tauri/src/lib.rs` (+1)
- `src/pages/TunnelManagementView/index.jsx` (+167/-16)

### docs(readme): 重构 README 文档结构与内容
- 重新组织文档结构，分为功能特性、技术栈、安装指南等清晰章节
- 补充应用界面预览和详细功能描述
- 增加 macOS 用户首次运行应用的解决方案
- 更新项目目录结构和开发指引
- 添加安全性、贡献指南和相关资源链接

---

## 🔧 技术细节

### WireGuard 配置解析支持

**Interface 配置**:
- ✅ PrivateKey - 本地私钥
- ✅ Address - 本地 IP 地址
- ✅ DNS - DNS 服务器
- ✅ MTU - 最大传输单元

**Peer 配置**:
- ✅ PublicKey - 对端公钥
- ✅ PresharedKey / PreSharedKey - 预共享密钥（两种格式支持）
- ✅ Endpoint - 对端连接地址
- ✅ AllowedIPs - 允许的 IP 范围
- ✅ PersistentKeepalive - 保活间隔

### 二维码识别

- 使用纯 JavaScript 库 `jsqr` 实现，无需服务器支持
- 支持多种图像格式自动转换
- 完整的错误处理：
  - 无法识别二维码时提示用户
  - 缺少必要字段时验证并告知

---

## 🐛 已知问题 & 注意事项

无新增已知问题

---

## 🙏 贡献者

- @mrtian2016

---

## 📦 依赖更新

### 新增依赖
- `jsqr@1.4.0` - 前端二维码识别库

### Rust 依赖
- 已有的 `base64` 库用于 Base64 编码

---

## 🚀 后续计划

- [ ] 支持从摄像头扫描二维码（移动端可用）
- [ ] 增强配置导入的验证和错误提示
- [ ] 支持拖放导入配置文件
- [ ] 配置导入历史记录

---

## 📥 升级指南

### 从 0.2.3 升级到 0.2.4

1. 下载最新版本安装包
2. 卸载旧版本
3. 安装新版本
4. 可选：删除旧的临时配置文件

升级过程中所有已保存的隧道配置和历史记录都会被保留。

---

## 🔐 安全性

- 所有文件读取操作都在本地进行，无网络传输
- 二维码扫描完全离线，不涉及第三方服务
- 隐私友好：配置信息仅存储在本地设备

---

**版本号**: v0.2.4
**发布时间**: 2025-10-24
**下一版本计划**: v0.2.5 (ETA: 待定)
