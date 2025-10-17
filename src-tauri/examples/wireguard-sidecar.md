# WireGuard-Go Sidecar 使用指南

本项目已集成 wireguard-go 作为 Tauri Sidecar，以下是使用方法。

## 配置说明

### 1. 二进制文件位置
所有平台的 wireguard-go 二进制文件位于 `src-tauri/binaries/` 目录：
- `wireguard-go-x86_64-apple-darwin` - macOS Intel
- `wireguard-go-aarch64-apple-darwin` - macOS ARM (Apple Silicon)
- `wireguard-go-x86_64-unknown-linux-gnu` - Linux x86_64
- `wireguard-go-x86_64-pc-windows-msvc.exe` - Windows x86_64

### 2. Tauri 配置
在 `tauri.conf.json` 中已配置：
```json
{
  "bundle": {
    "externalBin": [
      "binaries/wireguard-go"
    ]
  }
}
```

### 3. 权限配置
在 `capabilities/default.json` 中已添加权限：
```json
{
  "identifier": "shell:allow-execute",
  "allow": [
    {
      "name": "binaries/wireguard-go",
      "sidecar": true,
      "args": true
    }
  ]
}
```

## 前端调用示例 (React/JavaScript)

### 安装依赖
```bash
yarn add @tauri-apps/plugin-shell
```

### 示例代码

```javascript
import { Command } from '@tauri-apps/plugin-shell';
import { useState } from 'react';

function WireGuardManager() {
  const [output, setOutput] = useState('');
  const [isRunning, setIsRunning] = useState(false);

  // 检查 wireguard-go 版本
  const checkVersion = async () => {
    try {
      const command = Command.sidecar('binaries/wireguard-go', ['--version']);
      const result = await command.execute();

      if (result.code === 0) {
        setOutput(`版本信息：\n${result.stdout}`);
      } else {
        setOutput(`错误：\n${result.stderr}`);
      }
    } catch (error) {
      console.error('执行失败:', error);
      setOutput(`执行失败: ${error.message}`);
    }
  };

  // 启动 WireGuard 隧道
  const startTunnel = async (interfaceName = 'wg0') => {
    try {
      // wireguard-go 需要一个配置文件路径或接口名称
      const command = Command.sidecar('binaries/wireguard-go', [interfaceName]);

      // 使用 spawn 启动后台进程
      const child = await command.spawn();

      setIsRunning(true);
      setOutput(`隧道已启动，PID: ${child.pid}`);

      // 监听输出
      child.stdout.on('data', (line) => {
        console.log('stdout:', line);
      });

      child.stderr.on('data', (line) => {
        console.error('stderr:', line);
      });

      // 保存 child 引用以便后续停止
      return child;

    } catch (error) {
      console.error('启动失败:', error);
      setOutput(`启动失败: ${error.message}`);
    }
  };

  // 停止隧道
  const stopTunnel = async (child) => {
    try {
      if (child) {
        await child.kill();
        setIsRunning(false);
        setOutput('隧道已停止');
      }
    } catch (error) {
      console.error('停止失败:', error);
    }
  };

  return (
    <div>
      <h2>WireGuard 隧道管理</h2>
      <button onClick={checkVersion}>检查版本</button>
      <button onClick={() => startTunnel('wg0')} disabled={isRunning}>
        启动隧道
      </button>
      <pre>{output}</pre>
    </div>
  );
}

export default WireGuardManager;
```

## Rust 后端调用示例

在 `src-tauri/src/lib.rs` 中添加命令：

```rust
use tauri::Manager;
use tauri_plugin_shell::ShellExt;

#[tauri::command]
async fn start_wireguard(
    app: tauri::AppHandle,
    interface_name: String,
) -> Result<String, String> {
    let sidecar_command = app
        .shell()
        .sidecar("binaries/wireguard-go")
        .map_err(|e| format!("Failed to create sidecar command: {}", e))?;

    let (mut rx, child) = sidecar_command
        .args([&interface_name])
        .spawn()
        .map_err(|e| format!("Failed to spawn: {}", e))?;

    // 在后台监听输出
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                tauri_plugin_shell::process::CommandEvent::Stdout(line) => {
                    println!("[WG stdout] {}", String::from_utf8_lossy(&line));
                }
                tauri_plugin_shell::process::CommandEvent::Stderr(line) => {
                    eprintln!("[WG stderr] {}", String::from_utf8_lossy(&line));
                }
                tauri_plugin_shell::process::CommandEvent::Terminated(payload) => {
                    println!("[WG] Process terminated with code: {:?}", payload.code);
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(format!("WireGuard started with PID: {}", child.pid()))
}

#[tauri::command]
async fn check_wireguard_version(app: tauri::AppHandle) -> Result<String, String> {
    let output = app
        .shell()
        .sidecar("binaries/wireguard-go")
        .map_err(|e| format!("Failed to create sidecar: {}", e))?
        .args(["--version"])
        .output()
        .await
        .map_err(|e| format!("Failed to execute: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
```

别忘了在 `invoke_handler` 中注册这些命令：

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_shell::init())
    .invoke_handler(tauri::generate_handler![
        start_wireguard,
        check_wireguard_version
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

## 注意事项

1. **权限要求**：在 macOS 和 Linux 上，创建 TUN 设备需要 root 权限。你可能需要：
   - 使用 `sudo` 提升权限
   - 配置应用以管理员身份运行
   - 使用系统权限提示

2. **配置文件**：wireguard-go 需要配置文件，通常位于 `/etc/wireguard/wg0.conf`，或通过其他方式提供配置

3. **进程管理**：需要妥善管理 wireguard-go 进程的生命周期，确保应用退出时正确清理

4. **日志处理**：建议实现日志收集机制，便于调试和监控隧道状态

## 下一步

1. 实现配置文件管理功能
2. 添加隧道状态监控
3. 实现自动重连机制
4. 添加流量统计功能
