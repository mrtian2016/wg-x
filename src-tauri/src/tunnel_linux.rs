use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use crate::daemon_ipc::{IpcClient, PeerConfigIpc, TunnelConfigIpc};
use crate::tunnel::{
    base64_to_hex, generate_interface_name, interface_exists, parse_interface_status,
    resolve_endpoint, InterfaceConfig, PeerConfig, ProcessHandle, TunnelConfig,
    TunnelStatus, TUNNEL_CONFIGS, TUNNEL_PROCESSES,
};

// Linux: 使用守护进程方式管理 WireGuard (新方法)
// 通过 Unix Socket 与 root 守护进程通信
pub fn start_wireguard_linux_daemon(
    config: &InterfaceConfig,
    tunnel_id: &str,
    interface: &str,
    address: &str,
    wireguard_go_path: &str,
) -> Result<ProcessHandle, String> {
    log::info!("使用守护进程启动 WireGuard 隧道 (Linux)...");
    log::info!("传递给守护进程的 wireguard-go 路径: {}", wireguard_go_path);

    // 检查守护进程是否运行
    if !IpcClient::is_daemon_running() {
        return Err(
            "WireGuard 守护进程未运行。请先启动守护进程: sudo systemctl start wire-vault-daemon"
                .to_string(),
        );
    }

    // 构建 IPC 配置
    let peers: Vec<PeerConfigIpc> = config
        .peers
        .iter()
        .map(|p| PeerConfigIpc {
            public_key: p.public_key.clone(),
            endpoint: p.endpoint.clone(),
            allowed_ips: p.allowed_ips.clone(),
            persistent_keepalive: p.persistent_keepalive,
            preshared_key: p.preshared_key.clone(),
        })
        .collect();

    let tunnel_config = TunnelConfigIpc {
        tunnel_id: tunnel_id.to_string(),
        interface_name: interface.to_string(),
        private_key: config.private_key.clone(),
        address: address.to_string(),
        listen_port: config.listen_port,
        peers,
        wireguard_go_path: wireguard_go_path.to_string(),
        socket_dir: None, // 使用默认的 /var/run/wireguard
    };

    // 发送启动请求
    IpcClient::start_tunnel(tunnel_config)?;

    log::info!("隧道已通过守护进程启动");

    // 返回一个特殊的进程句柄,表示由守护进程管理
    // 使用 PID = -1 表示守护进程管理的隧道
    Ok(ProcessHandle::PrivilegedProcess(-1))
}

// Linux: 通过 pkexec 或 sudo 获取权限并一次性完成所有配置(旧方法,保留作为备用)
// pkexec 会弹出图形界面授权对话框,类似 macOS 的 osascript
pub fn start_wireguard_linux_legacy(
    wg_path: &str,
    interface: &str,
    address: &str,
    routes: &[String],
) -> Result<ProcessHandle, String> {
    log::info!("准备启动 WireGuard 隧道 (Linux)...");

    // 获取当前用户
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());

    // 转义路径和参数
    let escaped_wg_path = wg_path.replace('\'', "'\\''");
    let escaped_interface = interface.replace('\'', "'\\''");
    let escaped_user = user.replace('\'', "'\\''");
    let escaped_address = address.replace('\'', "'\\''");

    // Linux 方案:以 root 运行 wireguard-go,然后手动修改 socket 目录权限
    // 关键:在 wireguard-go 启动前就设置好目录权限
    let mut shell_script = format!(
        "'{}' -f '{}' > /tmp/wireguard-go.log 2>&1 & WG_PID=$! && sleep 2 && /sbin/ip address add '{}' dev '{}' && /sbin/ip link set '{}' up",
        escaped_wg_path, escaped_interface,
        escaped_address, escaped_interface, escaped_interface
    );

    // 添加路由
    for route in routes {
        if route == "0.0.0.0/0" || route == "::/0" {
            continue;
        }
        let escaped_route = route.replace('\'', "'\\''");
        shell_script.push_str(&format!(
            " && (/sbin/ip route delete '{}' > /dev/null 2>&1 || true) && (/sbin/ip route add '{}' dev '{}' > /dev/null 2>&1 || true)",
            escaped_route, escaped_route, escaped_interface
        ));
    }

    shell_script.push_str(" && echo $WG_PID");

    log::info!("执行命令脚本");

    // 尝试使用 pkexec (图形界面授权)
    let use_pkexec = std::process::Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let output = if use_pkexec {
        log::info!("使用 pkexec 请求管理员权限...");
        std::process::Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(&shell_script)
            .output()
            .map_err(|e| format!("执行命令失败: {}", e))?
    } else {
        log::info!("使用 sudo 请求管理员权限(可能需要在终端输入密码)...");
        std::process::Command::new("sudo")
            .arg("sh")
            .arg("-c")
            .arg(&shell_script)
            .output()
            .map_err(|e| format!("执行命令失败: {}", e))?
    };

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("启动隧道失败: {}", error_msg));
    }

    // 解析返回的 PID
    let pid_str = String::from_utf8_lossy(&output.stdout);
    let pid: i32 = pid_str
        .trim()
        .parse()
        .map_err(|e| format!("解析 PID 失败: {} (输出: {})", e, pid_str))?;

    log::info!("wireguard-go 已启动,PID: {}", pid);

    // 返回包含 PID 的进程句柄
    // 注意: Linux 使用特殊的标记来表示这是通过权限提升启动的进程
    Ok(ProcessHandle::PrivilegedProcess(pid))
}

// 停止 Linux 隧道 (守护进程方式)
pub fn stop_wireguard_linux(pid: i32, tunnel_id: &str) -> Result<(), String> {
    // 如果 PID == -1,说明是守护进程管理的隧道
    if pid == -1 {
        log::info!("通过守护进程停止隧道: {}", tunnel_id);
        return IpcClient::stop_tunnel(tunnel_id);
    }

    // 否则使用旧方法 (pkexec/sudo)
    log::info!("请求管理员权限以停止隧道进程 (PID: {})...", pid);

    // 尝试使用 pkexec
    let use_pkexec = std::process::Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let output = if use_pkexec {
        std::process::Command::new("pkexec")
            .arg("kill")
            .arg("-9")
            .arg(pid.to_string())
            .output()
            .map_err(|e| format!("执行命令失败: {}", e))?
    } else {
        std::process::Command::new("sudo")
            .arg("kill")
            .arg("-9")
            .arg(pid.to_string())
            .output()
            .map_err(|e| format!("执行命令失败: {}", e))?
    };

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("终止进程失败: {}", error_msg));
    }

    Ok(())
}

// Linux 实现：配置接口（通过 UAPI）
pub async fn configure_interface(
    interface: String,
    config: InterfaceConfig,
) -> Result<String, String> {
    let socket_path = format!("/var/run/wireguard/{}.sock", interface);

    let mut stream =
        UnixStream::connect(&socket_path).map_err(|e| format!("无法连接到 socket: {}", e))?;

    let mut uapi_config = String::from("set=1\n");

    // 将 Base64 私钥转换为十六进制
    let private_key_hex = base64_to_hex(&config.private_key)?;
    uapi_config.push_str(&format!("private_key={}\n", private_key_hex));

    if let Some(port) = config.listen_port {
        uapi_config.push_str(&format!("listen_port={}\n", port));
    }

    if let Some(fwmark) = config.fwmark {
        uapi_config.push_str(&format!("fwmark={}\n", fwmark));
    }

    if config.replace_peers {
        uapi_config.push_str("replace_peers=true\n");
    }

    // Peer 配置
    for peer in config.peers {
        let public_key_hex = base64_to_hex(&peer.public_key)?;
        uapi_config.push_str(&format!("public_key={}\n", public_key_hex));

        if let Some(endpoint) = peer.endpoint {
            if !endpoint.is_empty() {
                match resolve_endpoint(&endpoint) {
                    Ok(resolved_endpoint) => {
                        log::info!("解析 endpoint {} -> {}", endpoint, resolved_endpoint);
                        uapi_config.push_str(&format!("endpoint={}\n", resolved_endpoint));
                    }
                    Err(e) => {
                        return Err(format!("无法解析 endpoint {}: {}", endpoint, e));
                    }
                }
            }
        }

        if let Some(ref psk) = peer.preshared_key {
            if !psk.is_empty() {
                if psk == &peer.public_key {
                    return Err("预共享密钥不能与公钥相同,请重新生成或留空".to_string());
                }
                match base64_to_hex(psk) {
                    Ok(psk_hex) => {
                        uapi_config.push_str(&format!("preshared_key={}\n", psk_hex));
                    }
                    Err(e) => {
                        log::warn!("警告: 预共享密钥格式无效,已跳过: {}", e);
                    }
                }
            }
        }

        if let Some(keepalive) = peer.persistent_keepalive {
            uapi_config.push_str(&format!("persistent_keepalive_interval={}\n", keepalive));
        }

        for allowed_ip in peer.allowed_ips {
            uapi_config.push_str(&format!("allowed_ip={}\n", allowed_ip));
        }
    }

    uapi_config.push_str("\n");

    stream
        .write_all(uapi_config.as_bytes())
        .map_err(|e| format!("配置写入失败: {}", e))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if response.contains("errno=") && !response.contains("errno=0") {
        Err(format!("配置失败: {}", response))
    } else {
        Ok("配置应用成功".to_string())
    }
}

// Linux: 获取接口状态
pub async fn get_interface_status(_interface: String) -> Result<String, String> {
    // Linux 守护进程模式下，普通用户无法访问 root 创建的 socket
    // 需要通过 IPC 获取状态
    Err("Linux 平台请使用守护进程 IPC 获取状态".to_string())
}

// Linux: 获取隧道状态的实现
pub async fn get_tunnel_status_impl(
    tunnel_id: &str,
    _interface_name: &str,
) -> (u64, u64, Option<i64>) {
    log::info!("通过守护进程获取接口状态...");
    let tunnel_id = tunnel_id.to_string();
    // 使用 spawn_blocking 避免阻塞异步运行时
    let result = tokio::task::spawn_blocking(move || IpcClient::get_tunnel_status(&tunnel_id)).await;

    match result {
        Ok(Ok(status)) => {
            log::info!("获取状态成功");
            (status.tx_bytes, status.rx_bytes, status.last_handshake)
        }
        Ok(Err(e)) => {
            log::warn!("获取状态失败: {}", e);
            (0, 0, None)
        }
        Err(e) => {
            log::warn!("任务执行失败: {}", e);
            (0, 0, None)
        }
    }
}

// Linux: 启动隧道的平台特定部分
pub async fn start_tunnel_platform(
    tunnel_id: String,
    _tunnel_config: &TunnelConfig,
    interface_config: &InterfaceConfig,
    interface_name: String,
    _all_routes: Vec<String>,
    sidecar_path_str: &str,
) -> Result<(), String> {
    let process_handle = start_wireguard_linux_daemon(
        interface_config,
        &tunnel_id,
        &interface_name,
        &_tunnel_config.address,
        sidecar_path_str,
    )
    .map_err(|e| format!("启动隧道失败: {}", e))?;

    // 保存进程句柄
    {
        let mut processes = TUNNEL_PROCESSES.lock().await;
        processes.insert(tunnel_id.clone(), process_handle);
    }

    // 守护进程已经完成了所有配置工作（接口配置、IP地址、路由等）
    // GUI 应用不需要再做任何配置
    log::info!("隧道已通过守护进程启动并配置完成");

    // 注意：在 Linux 守护进程模式下，不启动 endpoint 刷新任务
    // 因为普通用户无法访问 root 创建的 socket
    // 如果需要支持动态域名，应该在守护进程内部实现 endpoint 刷新逻辑

    log::info!("隧道启动完成: {}", interface_name);
    Ok(())
}

// Linux: 停止隧道的清理逻辑
pub async fn cleanup_stale_tunnel(interface_name: &str) -> Result<(), String> {
    // 使用 pkexec 或 sudo 请求管理员权限来杀死进程
    let shell_command = format!("/usr/bin/pkill -9 -f 'wireguard-go.*{}'", interface_name);

    // 尝试使用 pkexec
    let use_pkexec = std::process::Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    log::info!("请求管理员权限以停止隧道...");

    let output = if use_pkexec {
        std::process::Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(&shell_command)
            .output()
    } else {
        std::process::Command::new("sudo")
            .arg("sh")
            .arg("-c")
            .arg(&shell_command)
            .output()
    };

    match output {
        Ok(result) => {
            if result.status.success() {
                log::info!("已发送终止信号给 wireguard-go 进程");
            } else {
                let error_msg = String::from_utf8_lossy(&result.stderr);
                log::error!("终止进程失败: {}", error_msg);
            }
        }
        Err(e) => {
            log::error!("执行命令失败: {}", e);
        }
    }

    // 等待进程终止
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 检查接口是否已被清理
    if !interface_exists(interface_name) {
        log::info!("接口已成功清理");
        Ok(())
    } else {
        log::warn!("接口仍然存在,可能需要手动清理");
        Err(format!(
            "已尝试清理残留接口 {},但仍然存在。请检查系统进程或重启应用",
            interface_name
        ))
    }
}

// Linux 不需要 endpoint 刷新任务（守护进程处理）
pub fn start_endpoint_refresh_task(_tunnel_id: String, _interface: String) {
    // Linux 守护进程模式下，endpoint 刷新应该在守护进程内部实现
}
