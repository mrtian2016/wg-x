// daemon_install.rs - GUI 安装/管理守护进程
// 通过 pkexec 获取权限执行安装操作

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tauri::Manager;

const SYSTEMD_SERVICE_CONTENT: &str = r#"[Unit]
Description=WireVault 守护进程
Documentation=https://github.com/mrtian2016/wire-vault
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/wire-vault daemon
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
    let installed = Path::new("/usr/local/bin/wire-vault").exists()
        && Path::new("/etc/systemd/system/wire-vault-daemon.service").exists();

    let mut running = false;
    let mut enabled = false;

    if installed {
        // 检查是否运行
        if let Ok(output) = Command::new("systemctl")
            .args(["is-active", "wire-vault-daemon"])
            .output()
        {
            running = output.status.success();
        }

        // 检查是否启用
        if let Ok(output) = Command::new("systemctl")
            .args(["is-enabled", "wire-vault-daemon"])
            .output()
        {
            enabled = output.status.success();
        }
    }

    // 获取版本
    let version = if installed {
        if let Ok(output) = Command::new("/usr/local/bin/wire-vault")
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
    log::info!("========== 开始安装守护进程 ==========");

    // 检查运行环境
    let appimage = std::env::var("APPIMAGE").ok();
    let appimage_str = appimage.as_deref().unwrap_or("未检测");
    log::info!("运行环境: AppImage = {}", appimage_str);

    if appimage.is_some() {
        log::info!("✓ 检测到 AppImage 环境，安装脚本将从 AppImage 挂载点提取文件");
    }

    // 获取当前可执行文件路径
    let current_exe =
        std::env::current_exe().map_err(|e| {
            let msg = format!("获取当前执行文件路径失败: {}", e);
            log::error!("{}", msg);
            msg
        })?;

    let current_exe_str = current_exe.to_str().ok_or_else(|| {
        let msg = "无效的可执行文件路径".to_string();
        log::error!("{}", msg);
        msg
    })?;

    log::info!("应用可执行文件: {}", current_exe_str);

    // 获取 wireguard-go sidecar 的路径
    // 尝试多个可能的位置
    let sidecar_path = {
        // 1. 尝试 Resource 目录
        let resource_path = app
            .path()
            .resolve("wireguard-go", tauri::path::BaseDirectory::Resource)
            .ok();

        // 2. 尝试可执行文件同目录
        let exe_dir_path = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("wireguard-go")));

        // 检查并记录每个路径
        let found_resource = resource_path.as_ref().map(|p| p.exists()).unwrap_or(false);
        let found_exe_dir = exe_dir_path.as_ref().map(|p| p.exists()).unwrap_or(false);

        if let Some(ref path) = resource_path {
            if found_resource {
                log::info!("✓ 从 Resource 目录找到 sidecar: {:?}", path);
            } else {
                log::warn!("Resource 目录路径不存在: {:?}", path);
            }
        }

        if let Some(ref path) = exe_dir_path {
            if found_exe_dir {
                log::info!("✓ 从可执行文件目录找到 sidecar: {:?}", path);
            } else {
                log::warn!("可执行文件目录路径不存在: {:?}", path);
            }
        }

        // 选择第一个存在的路径
        if found_resource {
            resource_path.unwrap()
        } else if found_exe_dir {
            exe_dir_path.unwrap()
        } else {
            let msg = "无法找到 wireguard-go 文件".to_string();
            log::error!("{}", msg);
            log::error!("尝试的路径:");
            if let Some(p) = resource_path {
                log::error!("  - {:?} (不存在)", p);
            }
            if let Some(p) = exe_dir_path {
                log::error!("  - {:?} (不存在)", p);
            }
            return Err(msg);
        }
    };

    let sidecar_path_str = sidecar_path
        .to_str()
        .ok_or_else(|| {
            let msg = "无法转换 sidecar 路径".to_string();
            log::error!("{}", msg);
            msg
        })?;

    log::info!("sidecar 路径: {}", sidecar_path_str);

    // 准备临时目录用于存放文件
    let temp_dir = "/tmp/wire-vault-install-temp";
    log::info!("创建临时目录: {}", temp_dir);
    fs::create_dir_all(temp_dir).map_err(|e| {
        let msg = format!("创建临时目录失败: {}", e);
        log::error!("{}", msg);
        msg
    })?;

    // 复制 wireguard-go 到临时目录
    let temp_sidecar = format!("{}/wireguard-go", temp_dir);
    if sidecar_path.exists() {
        log::info!("✓ sidecar 文件存在，复制到临时目录");
        fs::copy(&sidecar_path, &temp_sidecar).map_err(|e| {
            let msg = format!("复制 wireguard-go 到临时目录失败: {}", e);
            log::error!("{}", msg);
            msg
        })?;
        log::info!("✓ wireguard-go 已复制到: {}", temp_sidecar);

        // 设置可执行权限
        fs::set_permissions(&temp_sidecar, fs::Permissions::from_mode(0o755))
            .map_err(|e| {
                let msg = format!("设置 wireguard-go 权限失败: {}", e);
                log::error!("{}", msg);
                msg
            })?;
    } else {
        let msg = format!("sidecar 文件不存在: {}", sidecar_path_str);
        log::error!("{}", msg);
        // 清理临时目录
        let _ = fs::remove_dir_all(temp_dir);
        return Err(msg);
    }

    // 复制应用可执行文件到临时目录
    let temp_app = format!("{}/wire_vault", temp_dir);
    log::info!("复制应用到临时目录: {}", temp_app);
    fs::copy(&current_exe, &temp_app).map_err(|e| {
        let msg = format!("复制应用到临时目录失败: {}", e);
        log::error!("{}", msg);
        // 清理临时目录
        let _ = fs::remove_dir_all(temp_dir);
        msg
    })?;
    log::info!("✓ 应用已复制到: {}", temp_app);

    // 设置可执行权限
    fs::set_permissions(&temp_app, fs::Permissions::from_mode(0o755))
        .map_err(|e| {
            let msg = format!("设置应用权限失败: {}", e);
            log::error!("{}", msg);
            // 清理临时目录
            let _ = fs::remove_dir_all(temp_dir);
            msg
        })?;

    // 创建临时安装脚本
    let script_content = format!(
        r#"#!/bin/bash
set -e

# 详细日志函数
log_info() {{
    echo "[INFO] $1"
}}

log_error() {{
    echo "[ERROR] $1" >&2
}}

log_info "========== WireVault 守护进程安装开始 =========="
log_info "临时目录: {}"

# 1. 创建 /opt/wire-vault 目录并复制 wireguard-go
log_info "[1/5] 创建目录并复制 wireguard-go..."
mkdir -p /opt/wire-vault
log_info "  ✓ 目录 /opt/wire-vault 已创建"

SIDECAR_SOURCE="{}"
log_info "  源文件: $SIDECAR_SOURCE"

if [ ! -r "$SIDECAR_SOURCE" ]; then
    log_error "  ✗ 无法读取源文件: $SIDECAR_SOURCE"
    exit 1
fi

if install -m 755 "$SIDECAR_SOURCE" /opt/wire-vault/wireguard-go; then
    log_info "  ✓ wireguard-go 已复制到 /opt/wire-vault"
    log_info "  文件权限: $(stat -c '%a' /opt/wire-vault/wireguard-go)"
else
    log_error "  ✗ 复制 wireguard-go 失败"
    exit 1
fi

# 2. 复制主可执行文件
log_info "[2/5] 复制可执行文件..."
APP_SOURCE="{}"
log_info "  源文件: $APP_SOURCE"

if [ ! -r "$APP_SOURCE" ]; then
    log_error "  ✗ 无法读取源文件: $APP_SOURCE"
    exit 1
fi

if install -m 755 "$APP_SOURCE" /usr/local/bin/wire-vault; then
    log_info "  ✓ 应用已复制到 /usr/local/bin/wire-vault"
    log_info "  文件权限: $(stat -c '%a' /usr/local/bin/wire-vault)"
else
    log_error "  ✗ 复制应用文件失败"
    exit 1
fi

# 3. 创建 systemd service 文件
log_info "[3/5] 创建 systemd service..."
if cat > /etc/systemd/system/wire-vault-daemon.service << 'SERVICEEOF'
{}SERVICEEOF
then
    log_info "  ✓ systemd service 文件已创建"
    chmod 644 /etc/systemd/system/wire-vault-daemon.service
    log_info "  文件权限: $(stat -c '%a' /etc/systemd/system/wire-vault-daemon.service)"
else
    log_error "  ✗ 创建 systemd service 失败"
    exit 1
fi

# 4. 重新加载 systemd
log_info "[4/5] 重新加载 systemd..."
if systemctl daemon-reload; then
    log_info "  ✓ systemd 已重新加载"
else
    log_error "  ✗ systemd 重新加载失败"
    exit 1
fi

# 5. 启动并启用守护进程
log_info "[5/5] 启动守护进程..."
if systemctl enable wire-vault-daemon; then
    log_info "  ✓ 守护进程已启用"
else
    log_error "  ✗ 启用守护进程失败"
    exit 1
fi

if systemctl start wire-vault-daemon; then
    log_info "  ✓ 守护进程已启动"
else
    log_error "  ✗ 启动守护进程失败"
    exit 1
fi

# 验证
log_info "验证守护进程状态..."
sleep 2

if systemctl is-active --quiet wire-vault-daemon; then
    log_info "✓ 守护进程安装并启动成功!"
    log_info "守护进程状态:"
    systemctl status wire-vault-daemon --no-pager
    exit 0
else
    log_error "✗ 守护进程启动失败"
    log_error "最近 30 条日志:"
    journalctl -u wire-vault-daemon -n 30 --no-pager || true
    log_error "systemd 状态:"
    systemctl status wire-vault-daemon --no-pager || true
    exit 1
fi
"#,
        temp_dir, temp_sidecar, temp_app, SYSTEMD_SERVICE_CONTENT
    );

    log::info!("安装脚本已生成，长度: {} 字节", script_content.len());

    // 写入临时脚本
    let script_path = "/tmp/wire-vault-install-daemon.sh";
    fs::write(script_path, script_content).map_err(|e| {
        let msg = format!("创建安装脚本失败: {}", e);
        log::error!("{}", msg);
        msg
    })?;
    log::info!("安装脚本已写入: {}", script_path);

    // 设置执行权限
    fs::set_permissions(script_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| {
            let msg = format!("设置脚本权限失败: {}", e);
            log::error!("{}", msg);
            msg
        })?;
    log::info!("脚本权限已设置为 0755");

    // 使用 pkexec 执行安装脚本
    log::info!("请求管理员权限以安装守护进程...");
    log::info!("执行命令: pkexec sh {}", script_path);

    let output = Command::new("pkexec")
        .arg("sh")
        .arg(script_path)
        .output()
        .map_err(|e| {
            let msg = format!("执行安装脚本失败: {}。请确保已安装 pkexec (polkit)", e);
            log::error!("{}", msg);
            msg
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    log::info!("脚本执行返回码: {}", output.status.code().unwrap_or(-1));
    log::info!("脚本 stdout:\n{}", stdout);
    if !stderr.is_empty() {
        log::warn!("脚本 stderr:\n{}", stderr);
    }

    // 清理临时脚本
    if let Err(e) = fs::remove_file(script_path) {
        log::warn!("清理临时脚本失败: {}", e);
    } else {
        log::info!("临时脚本已清理");
    }

    // 清理临时目录
    if let Err(e) = fs::remove_dir_all(temp_dir) {
        log::warn!("清理临时目录失败: {}", e);
    } else {
        log::info!("临时目录已清理: {}", temp_dir);
    }

    if !output.status.success() {
        if stderr.contains("dismissed") || stderr.contains("canceled") || stderr.contains("Authentication required") {
            let msg = "用户取消了授权或身份验证失败".to_string();
            log::warn!("{}", msg);
            return Err(msg);
        }
        let msg = format!("安装失败:\n{}", stderr);
        log::error!("{}", msg);
        return Err(msg);
    }

    log::info!("========== 守护进程安装完成 ==========");
    Ok(stdout.to_string())
}

/// 卸载守护进程
#[tauri::command]
pub async fn uninstall_daemon() -> Result<String, String> {
    let script_content = r#"#!/bin/bash
set -e

echo "=== WireVault 守护进程卸载 ==="

# 1. 停止并禁用服务
echo "[1/5] 停止服务..."
systemctl stop wire-vault-daemon || true
systemctl disable wire-vault-daemon || true

# 2. 删除 systemd service 文件
echo "[2/5] 删除 systemd service..."
rm -f /etc/systemd/system/wire-vault-daemon.service

# 3. 重新加载 systemd
echo "[3/5] 重新加载 systemd..."
systemctl daemon-reload

# 4. 删除可执行文件和配置目录
echo "[4/5] 删除可执行文件..."
rm -f /usr/local/bin/wire-vault

# 5. 清理 /opt/wire-vault 目录
echo "[5/5] 清理配置目录..."
rm -rf /opt/wire-vault

# 清理 socket 文件
rm -f /var/run/wire-vault-daemon.sock

echo "✓ 守护进程已卸载"
"#;

    // 写入临时脚本
    let script_path = "/tmp/wire-vault-uninstall-daemon.sh";
    fs::write(script_path, script_content).map_err(|e| format!("创建卸载脚本失败: {}", e))?;

    fs::set_permissions(script_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("设置脚本权限失败: {}", e))?;

    // 使用 pkexec 执行卸载脚本
    log::info!("请求管理员权限以卸载守护进程...");

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

/// 辅助函数: 执行 pkexec 命令并确保环境变量正确
fn run_pkexec_systemctl(
    action: &str,
    service: &str,
) -> Result<std::process::Output, std::io::Error> {
    log::info!("执行 pkexec systemctl {} {}", action, service);

    let mut cmd = Command::new("pkexec");
    cmd.args(["systemctl", action, service]);

    // 确保环境变量传递 (用于图形化认证对话框)
    if let Ok(display) = std::env::var("DISPLAY") {
        log::info!("设置 DISPLAY={}", display);
        cmd.env("DISPLAY", display);
    }

    if let Ok(xauth) = std::env::var("XAUTHORITY") {
        log::info!("设置 XAUTHORITY={}", xauth);
        cmd.env("XAUTHORITY", xauth);
    }

    if let Ok(wayland) = std::env::var("WAYLAND_DISPLAY") {
        log::info!("设置 WAYLAND_DISPLAY={}", wayland);
        cmd.env("WAYLAND_DISPLAY", wayland);
    }

    log::info!("开始执行命令...");
    let result = cmd.output();
    log::info!("命令执行完成");
    result
}

/// 启动守护进程 (使用 pkexec 请求授权)
#[tauri::command]
pub async fn start_daemon_service() -> Result<(), String> {
    log::info!("start_daemon_service 被调用");

    // 使用 spawn_blocking 避免阻塞异步运行时
    let output = tokio::task::spawn_blocking(|| run_pkexec_systemctl("start", "wire-vault-daemon"))
        .await
        .map_err(|e| format!("任务执行失败: {}", e))?
        .map_err(|e| format!("启动服务失败: {}", e))?;

    log::info!("命令执行结果: status={:?}", output.status);

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        log::error!("错误输出: {}", error_msg);

        if error_msg.contains("dismissed") || error_msg.contains("canceled") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("启动服务失败: {}", error_msg));
    }

    log::info!("守护进程启动成功");
    Ok(())
}

/// 停止守护进程 (使用 pkexec 请求授权)
#[tauri::command]
pub async fn stop_daemon_service() -> Result<(), String> {
    log::info!("stop_daemon_service 被调用");

    // 使用 spawn_blocking 避免阻塞异步运行时
    let output = tokio::task::spawn_blocking(|| run_pkexec_systemctl("stop", "wire-vault-daemon"))
        .await
        .map_err(|e| format!("任务执行失败: {}", e))?
        .map_err(|e| format!("停止服务失败: {}", e))?;

    log::info!("命令执行结果: status={:?}", output.status);

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        log::error!("错误输出: {}", error_msg);

        if error_msg.contains("dismissed") || error_msg.contains("canceled") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("停止服务失败: {}", error_msg));
    }

    log::info!("守护进程停止成功");
    Ok(())
}

/// 重启守护进程 (使用 pkexec 请求授权)
#[tauri::command]
pub async fn restart_daemon_service() -> Result<(), String> {
    log::info!("restart_daemon_service 被调用");

    // 使用 spawn_blocking 避免阻塞异步运行时
    let output = tokio::task::spawn_blocking(|| run_pkexec_systemctl("restart", "wire-vault-daemon"))
        .await
        .map_err(|e| format!("任务执行失败: {}", e))?
        .map_err(|e| format!("重启服务失败: {}", e))?;

    log::info!("命令执行结果: status={:?}", output.status);

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        log::error!("错误输出: {}", error_msg);

        if error_msg.contains("dismissed") || error_msg.contains("canceled") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("重启服务失败: {}", error_msg));
    }

    log::info!("守护进程重启成功");
    Ok(())
}

/// 启用开机自动启动 (使用 pkexec 请求授权)
#[tauri::command]
pub async fn enable_daemon_service() -> Result<(), String> {
    log::info!("enable_daemon_service 被调用");

    // 使用 spawn_blocking 避免阻塞异步运行时
    let output = tokio::task::spawn_blocking(|| run_pkexec_systemctl("enable", "wire-vault-daemon"))
        .await
        .map_err(|e| format!("任务执行失败: {}", e))?
        .map_err(|e| format!("启用服务失败: {}", e))?;

    log::info!("命令执行结果: status={:?}", output.status);

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        log::error!("错误输出: {}", error_msg);

        if error_msg.contains("dismissed") || error_msg.contains("canceled") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("启用服务失败: {}", error_msg));
    }

    log::info!("开机自启动已启用");
    Ok(())
}

/// 禁用开机自动启动 (使用 pkexec 请求授权)
#[tauri::command]
pub async fn disable_daemon_service() -> Result<(), String> {
    log::info!("disable_daemon_service 被调用");

    // 使用 spawn_blocking 避免阻塞异步运行时
    let output = tokio::task::spawn_blocking(|| run_pkexec_systemctl("disable", "wire-vault-daemon"))
        .await
        .map_err(|e| format!("任务执行失败: {}", e))?
        .map_err(|e| format!("禁用服务失败: {}", e))?;

    log::info!("命令执行结果: status={:?}", output.status);

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        log::error!("错误输出: {}", error_msg);

        if error_msg.contains("dismissed") || error_msg.contains("canceled") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("禁用服务失败: {}", error_msg));
    }

    log::info!("开机自启动已禁用");
    Ok(())
}

/// 获取守护进程日志
#[tauri::command]
pub async fn get_daemon_logs(lines: Option<usize>) -> Result<String, String> {
    let line_count = lines.unwrap_or(50);

    let output = Command::new("journalctl")
        .args([
            "-u",
            "wire-vault-daemon",
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
