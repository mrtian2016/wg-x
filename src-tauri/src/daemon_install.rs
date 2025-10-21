// daemon_install.rs - GUI 安装/管理守护进程
// 通过 pkexec 获取权限执行安装操作

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tauri::Manager;

const SYSTEMD_SERVICE_CONTENT: &str = r#"[Unit]
Description=WireGuard X 守护进程
Documentation=https://github.com/your-repo/wg-x
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/wg-x daemon
Restart=on-failure
RestartSec=5s

# 安全设置
NoNewPrivileges=false
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/run/wireguard /var/run

# 日志
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
"#;

/// 守护进程状态
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct DaemonStatus {
    pub installed: bool,
    pub running: bool,
    pub enabled: bool,
    pub version: Option<String>,
}

/// 检查守护进程状态
#[tauri::command]
pub async fn check_daemon_status() -> Result<DaemonStatus, String> {
    // 检查是否安装 (检查可执行文件和 systemd service)
    let installed = Path::new("/usr/local/bin/wg-x").exists()
        && Path::new("/etc/systemd/system/wg-x-daemon.service").exists();

    let mut running = false;
    let mut enabled = false;

    if installed {
        // 检查是否运行
        if let Ok(output) = Command::new("systemctl")
            .args(["is-active", "wg-x-daemon"])
            .output()
        {
            running = output.status.success();
        }

        // 检查是否启用
        if let Ok(output) = Command::new("systemctl")
            .args(["is-enabled", "wg-x-daemon"])
            .output()
        {
            enabled = output.status.success();
        }
    }

    // 获取版本
    let version = if installed {
        if let Ok(output) = Command::new("/usr/local/bin/wg-x")
            .arg("--version")
            .output()
        {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    } else {
        None
    };

    Ok(DaemonStatus {
        installed,
        running,
        enabled,
        version,
    })
}

/// 安装守护进程
/// 使用 pkexec 获取权限
#[tauri::command]
pub async fn install_daemon(app: tauri::AppHandle) -> Result<String, String> {
    // 获取当前可执行文件路径
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("获取当前执行文件路径失败: {}", e))?;

    let current_exe_str = current_exe
        .to_str()
        .ok_or("无效的可执行文件路径")?;

    // 获取 wireguard-go sidecar 的路径
    let sidecar_path = app
        .path()
        .resolve("wireguard-go", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("获取 sidecar 路径失败: {}", e))?;

    let sidecar_path_str = sidecar_path
        .to_str()
        .ok_or_else(|| "无法转换 sidecar 路径".to_string())?;

    // 创建临时安装脚本
    let script_content = format!(
        r#"#!/bin/bash
set -e

echo "=== WireGuard X 守护进程安装 ==="

# 1. 创建 /opt/wg-x 目录并复制 wireguard-go
echo "[1/5] 创建目录并复制 wireguard-go..."
mkdir -p /opt/wg-x
cp "{}" /opt/wg-x/wireguard-go
chmod 755 /opt/wg-x/wireguard-go

# 2. 复制主可执行文件
echo "[2/5] 复制可执行文件..."
cp "{}" /usr/local/bin/wg-x
chmod 755 /usr/local/bin/wg-x

# 3. 创建 systemd service 文件
echo "[3/5] 创建 systemd service..."
cat > /etc/systemd/system/wg-x-daemon.service << 'SERVICEEOF'
{}SERVICEEOF

chmod 644 /etc/systemd/system/wg-x-daemon.service

# 4. 重新加载 systemd
echo "[4/5] 重新加载 systemd..."
systemctl daemon-reload

# 5. 启动并启用守护进程
echo "[5/5] 启动守护进程..."
systemctl enable wg-x-daemon
systemctl start wg-x-daemon

# 验证
sleep 2
if systemctl is-active --quiet wg-x-daemon; then
    echo "✓ 守护进程安装并启动成功!"
    exit 0
else
    echo "✗ 守护进程启动失败"
    journalctl -u wg-x-daemon -n 20
    exit 1
fi
"#,
        sidecar_path_str, current_exe_str, SYSTEMD_SERVICE_CONTENT
    );

    // 写入临时脚本
    let script_path = "/tmp/wg-x-install-daemon.sh";
    fs::write(script_path, script_content)
        .map_err(|e| format!("创建安装脚本失败: {}", e))?;

    // 设置执行权限
    fs::set_permissions(script_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("设置脚本权限失败: {}", e))?;

    // 使用 pkexec 执行安装脚本
    println!("请求管理员权限以安装守护进程...");

    let output = Command::new("pkexec")
        .arg("sh")
        .arg(script_path)
        .output()
        .map_err(|e| format!("执行安装脚本失败: {}。请确保已安装 pkexec (polkit)", e))?;

    // 清理临时脚本
    let _ = fs::remove_file(script_path);

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        if error_msg.contains("dismissed") || error_msg.contains("canceled") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("安装失败: {}", error_msg));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.to_string())
}

/// 卸载守护进程
#[tauri::command]
pub async fn uninstall_daemon() -> Result<String, String> {
    let script_content = r#"#!/bin/bash
set -e

echo "=== WireGuard X 守护进程卸载 ==="

# 1. 停止并禁用服务
echo "[1/5] 停止服务..."
systemctl stop wg-x-daemon || true
systemctl disable wg-x-daemon || true

# 2. 删除 systemd service 文件
echo "[2/5] 删除 systemd service..."
rm -f /etc/systemd/system/wg-x-daemon.service

# 3. 重新加载 systemd
echo "[3/5] 重新加载 systemd..."
systemctl daemon-reload

# 4. 删除可执行文件和配置目录
echo "[4/5] 删除可执行文件..."
rm -f /usr/local/bin/wg-x

# 5. 清理 /opt/wg-x 目录
echo "[5/5] 清理配置目录..."
rm -rf /opt/wg-x

# 清理 socket 文件
rm -f /var/run/wg-x-daemon.sock

echo "✓ 守护进程已卸载"
"#;

    // 写入临时脚本
    let script_path = "/tmp/wg-x-uninstall-daemon.sh";
    fs::write(script_path, script_content)
        .map_err(|e| format!("创建卸载脚本失败: {}", e))?;

    fs::set_permissions(script_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("设置脚本权限失败: {}", e))?;

    // 使用 pkexec 执行卸载脚本
    println!("请求管理员权限以卸载守护进程...");

    let output = Command::new("pkexec")
        .arg("sh")
        .arg(script_path)
        .output()
        .map_err(|e| format!("执行卸载脚本失败: {}", e))?;

    // 清理临时脚本
    let _ = fs::remove_file(script_path);

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        if error_msg.contains("dismissed") || error_msg.contains("canceled") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("卸载失败: {}", error_msg));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.to_string())
}

/// 启动守护进程
#[tauri::command]
pub async fn start_daemon_service() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["start", "wg-x-daemon"])
        .output()
        .map_err(|e| format!("启动服务失败: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "启动服务失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// 停止守护进程
#[tauri::command]
pub async fn stop_daemon_service() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["stop", "wg-x-daemon"])
        .output()
        .map_err(|e| format!("停止服务失败: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "停止服务失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// 重启守护进程
#[tauri::command]
pub async fn restart_daemon_service() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["restart", "wg-x-daemon"])
        .output()
        .map_err(|e| format!("重启服务失败: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "重启服务失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// 启用开机自动启动
#[tauri::command]
pub async fn enable_daemon_service() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["enable", "wg-x-daemon"])
        .output()
        .map_err(|e| format!("启用服务失败: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "启用服务失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// 禁用开机自动启动
#[tauri::command]
pub async fn disable_daemon_service() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["disable", "wg-x-daemon"])
        .output()
        .map_err(|e| format!("禁用服务失败: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "禁用服务失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// 获取守护进程日志
#[tauri::command]
pub async fn get_daemon_logs(lines: Option<usize>) -> Result<String, String> {
    let line_count = lines.unwrap_or(50);

    let output = Command::new("journalctl")
        .args([
            "-u",
            "wg-x-daemon",
            "-n",
            &line_count.to_string(),
            "--no-pager",
        ])
        .output()
        .map_err(|e| format!("获取日志失败: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "获取日志失败: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
