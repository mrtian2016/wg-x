use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::tunnel::{InterfaceConfig, ProcessHandle, TunnelConfig, TUNNEL_PROCESSES};

// Windows 创建进程标志：CREATE_NO_WINDOW = 0x08000000
// 用于隐藏控制台窗口
const CREATE_NO_WINDOW: u32 = 0x08000000;

// Windows 工具函数：清理标识符
pub fn sanitize_identifier(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            result.push(ch);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() {
        "wgx_tunnel".to_string()
    } else {
        result
    }
}

// Windows: 查找 WireGuard 工具
fn locate_wireguard_tool(tool_name: &str) -> Option<PathBuf> {
    if let Some(path_var) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&path_var) {
            let candidate = path.join(tool_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    let mut candidates = Vec::new();

    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(program_files).join("WireGuard"));
    }
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        candidates.push(PathBuf::from(program_files_x86).join("WireGuard"));
    }

    candidates.push(PathBuf::from(r"C:\Program Files\WireGuard"));
    candidates.push(PathBuf::from(r"C:\Program Files (x86)\WireGuard"));

    if let Some(local_app) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local_app).join(r"Programs\WireGuard"));
    }

    for dir in candidates {
        let candidate = dir.join(tool_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

pub fn locate_wireguard_tools() -> Result<(PathBuf, PathBuf), String> {
    let wireguard = locate_wireguard_tool("wireguard.exe")
        .ok_or_else(|| "未找到 wireguard.exe，请先安装官方 WireGuard 客户端".to_string())?;
    let wg = locate_wireguard_tool("wg.exe")
        .ok_or_else(|| "未找到 wg.exe，请先安装官方 WireGuard 客户端".to_string())?;
    Ok((wireguard, wg))
}

fn split_config_values(value: &str) -> Vec<String> {
    value
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn build_windows_config_content(
    tunnel_config: &TunnelConfig,
    interface_config: &InterfaceConfig,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("[Interface]".to_string());
    lines.push(format!(
        "PrivateKey = {}",
        interface_config.private_key.trim()
    ));

    for address in split_config_values(&tunnel_config.address) {
        lines.push(format!("Address = {}", address));
    }

    if let Some(port) = interface_config.listen_port {
        lines.push(format!("ListenPort = {}", port));
    }

    if !tunnel_config.dns.trim().is_empty() {
        for dns in split_config_values(&tunnel_config.dns) {
            lines.push(format!("DNS = {}", dns));
        }
    }

    if !tunnel_config.mtu.trim().is_empty() {
        lines.push(format!("MTU = {}", tunnel_config.mtu.trim()));
    }

    lines.push(String::new());

    for peer in &interface_config.peers {
        lines.push("[Peer]".to_string());
        lines.push(format!("PublicKey = {}", peer.public_key.trim()));

        if let Some(ref psk) = peer.preshared_key {
            if !psk.trim().is_empty() {
                lines.push(format!("PresharedKey = {}", psk.trim()));
            }
        }

        if let Some(ref endpoint) = peer.endpoint {
            if !endpoint.trim().is_empty() {
                lines.push(format!("Endpoint = {}", endpoint.trim()));
            }
        }

        if !peer.allowed_ips.is_empty() {
            let ips = peer
                .allowed_ips
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            if !ips.is_empty() {
                lines.push(format!("AllowedIPs = {}", ips));
            }
        }

        if let Some(keepalive) = peer.persistent_keepalive {
            lines.push(format!("PersistentKeepalive = {}", keepalive));
        }

        lines.push(String::new());
    }

    lines.join("\r\n")
}

fn extract_service_name_from_output(output: &str) -> Option<String> {
    if let Some(pos) = output.find("WireGuardTunnel$") {
        let tail = &output[pos..];
        let service_name: String = tail
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '$' || *ch == '-' || *ch == '_')
            .collect();
        if service_name.starts_with("WireGuardTunnel$") {
            return Some(service_name);
        }
    }
    None
}

pub fn start_wireguard_windows(
    tunnel_id: &str,
    tunnel_config: &TunnelConfig,
    interface_config: &InterfaceConfig,
    tunnels_dir: &Path,
) -> Result<ProcessHandle, String> {
    log::info!("========== Windows 启动 WireGuard 隧道 ==========");
    log::info!("隧道 ID: {}", tunnel_id);

    let (wireguard_path, _wg_path) = locate_wireguard_tools()?;
    log::info!("WireGuard 工具路径: {:?}", wireguard_path);

    let sanitized_id = sanitize_identifier(tunnel_id);
    log::info!("清理后的 ID: {}", sanitized_id);

    let config_file_name = format!("wgx-{}.conf", sanitized_id);
    let config_path = tunnels_dir.join(config_file_name);
    log::info!("配置文件路径: {:?}", config_path);

    let config_content = build_windows_config_content(tunnel_config, interface_config);
    log::debug!("生成的配置内容:\n{}", config_content);

    std::fs::write(&config_path, &config_content)
        .map_err(|e| format!("写入 Windows 配置失败: {}", e))?;
    log::info!("配置文件已写入");

    // 启动前先尝试卸载同名服务，确保重复安装时不会失败
    let expected_service_name = format!("WireGuardTunnel${}", sanitized_id);
    log::info!("尝试卸载可能存在的旧服务: {}", expected_service_name);
    let _ = stop_wireguard_windows(&expected_service_name, &sanitized_id, Some(&config_path));

    log::info!("执行命令: {:?} /installtunnelservice {:?}", wireguard_path, config_path);
    let output = std::process::Command::new(&wireguard_path)
        .arg("/installtunnelservice")
        .arg(&config_path)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行 wireguard.exe 失败: {}", e))?;

    log::info!("命令执行完成，退出码: {:?}", output.status.code());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    log::info!("命令输出 (stdout):\n{}", stdout);
    if !stderr.is_empty() {
        log::warn!("命令错误输出 (stderr):\n{}", stderr);
    }

    if !output.status.success() {
        log::error!("安装隧道服务失败，退出码: {:?}", output.status.code());
        return Err(format!("安装隧道服务失败: {}", stderr.trim()));
    }

    let combined = format!("{}\n{}", stdout, stderr);
    let service_name =
        extract_service_name_from_output(&combined).unwrap_or(expected_service_name.clone());

    log::info!(
        "✅ WireGuard 隧道已安装为服务: {} (配置: {:?})",
        service_name,
        config_path
    );
    log::info!("================================================");

    Ok(ProcessHandle::WindowsService {
        service_name,
        interface_name: sanitized_id,
        config_path,
    })
}

pub fn stop_wireguard_windows(
    service_name: &str,
    interface_name: &str,
    config_path: Option<&Path>,
) -> Result<(), String> {
    log::info!("========== Windows 停止 WireGuard 隧道 ==========");
    log::info!("服务名称: {}", service_name);
    log::info!("接口名称: {}", interface_name);

    let (wireguard_path, _) = locate_wireguard_tools()?;
    log::info!("WireGuard 工具路径: {:?}", wireguard_path);

    let mut attempts = vec![service_name.to_string()];
    if let Some(path) = config_path {
        if let Some(path_str) = path.to_str() {
            attempts.push(path_str.to_string());
        }
    }

    if attempts.len() == 1 {
        // 当没有配置路径时，额外尝试使用 interface 名称
        attempts.push(interface_name.to_string());
    }

    let interface_stop_attempt = format!("WireGuardTunnel${}", interface_name);
    if interface_stop_attempt != service_name {
        attempts.push(interface_stop_attempt);
    }

    // 去重，保持尝试顺序
    let mut seen = HashSet::new();
    attempts.retain(|value| seen.insert(value.clone()));

    log::info!("将尝试卸载以下服务: {:?}", attempts);

    let mut last_error: Option<String> = None;

    for (index, target) in attempts.iter().enumerate() {
        log::info!("尝试 {}/{}: 卸载服务 {}", index + 1, attempts.len(), target);
        log::info!("执行命令: {:?} /uninstalltunnelservice {:?}", wireguard_path, target);

        let output = std::process::Command::new(&wireguard_path)
            .arg("/uninstalltunnelservice")
            .arg(target)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("执行 wireguard.exe 失败: {}", e))?;

        log::info!("命令执行完成，退出码: {:?}", output.status.code());

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.is_empty() {
            log::info!("命令输出 (stdout): {}", stdout.trim());
        }
        if !stderr.is_empty() {
            log::info!("命令输出 (stderr): {}", stderr.trim());
        }

        if output.status.success() {
            log::info!("✅ 已卸载 WireGuard 服务: {}", target);
            log::info!("================================================");
            return Ok(());
        }

        let message = format!("{}{}", stdout.trim(), stderr.trim());

        if message.is_empty()
            || message.contains("not found")
            || message.contains("不存在")
            || message.contains("未找到")
        {
            // 服务不存在，视为成功
            log::info!("WireGuard 服务 {} 已不存在", target);
            log::info!("================================================");
            return Ok(());
        }

        log::warn!("卸载失败: {}", message);
        last_error = Some(message);
    }

    log::error!("所有卸载尝试均失败");
    log::info!("================================================");

    if let Some(err) = last_error {
        Err(format!(
            "卸载 WireGuard 服务 {} 失败: {}",
            service_name, err
        ))
    } else {
        Err(format!("卸载 WireGuard 服务 {} 失败", service_name))
    }
}

fn parse_windows_dump(dump: &str) -> (u64, u64, Option<i64>) {
    let mut tx_total = 0u64;
    let mut rx_total = 0u64;
    let mut last_handshake: Option<i64> = None;

    for line in dump.lines() {
        let cols: Vec<&str> = line.split('\t').collect();

        // Peer 行至少包含 7 列
        if cols.len() >= 7 {
            // 常见格式: public_key, preshared, endpoint, allowed_ips, tx, rx, last_handshake, [nsec], persistent
            let tx = cols.get(4).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
            let rx = cols.get(5).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
            tx_total = tx_total.saturating_add(tx);
            rx_total = rx_total.saturating_add(rx);

            if let Some(sec) = cols.get(6).and_then(|v| v.parse::<i64>().ok()) {
                if sec > 0 {
                    last_handshake = Some(sec);
                }
            }
        }
    }

    (tx_total, rx_total, last_handshake)
}

pub fn get_windows_interface_counters(interface: &str) -> Result<(u64, u64, Option<i64>), String> {
    log::debug!("获取 Windows 接口统计信息: {}", interface);

    let (_, wg_path) = locate_wireguard_tools()?;
    log::debug!("wg.exe 路径: {:?}", wg_path);
    log::debug!("执行命令: {:?} show {} dump", wg_path, interface);

    let output = std::process::Command::new(&wg_path)
        .args(["show", interface, "dump"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行 wg.exe 失败: {}", e))?;

    log::debug!("命令执行完成，退出码: {:?}", output.status.code());

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("获取接口状态失败: {}", stderr.trim());
        return Err(format!("获取 WireGuard 接口状态失败: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    log::debug!("接口 dump 输出:\n{}", stdout);

    let result = parse_windows_dump(&stdout);
    log::debug!("解析结果: tx={}, rx={}, last_handshake={:?}", result.0, result.1, result.2);

    Ok(result)
}

// Windows 实现：配置接口
pub async fn configure_interface(
    _interface: String,
    _config: InterfaceConfig,
) -> Result<String, String> {
    Err("Windows 平台由官方 WireGuard 客户端管理配置，暂不支持通过应用直接下发".to_string())
}

// Windows: 获取接口状态
pub async fn get_interface_status(interface: String) -> Result<String, String> {
    let (_, wg_path) = locate_wireguard_tools()?;
    let output = std::process::Command::new(&wg_path)
        .args(["show", &interface, "dump"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行 wg.exe 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("获取 WireGuard 状态失败: {}", stderr.trim()))
    } else {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

// Windows: 获取隧道状态的实现
pub async fn get_tunnel_status_impl(
    _tunnel_id: &str,
    interface_name: &str,
) -> (u64, u64, Option<i64>) {
    get_windows_interface_counters(interface_name).unwrap_or((0, 0, None))
}

// Windows: 启动隧道的平台特定部分
pub async fn start_tunnel_platform(
    tunnel_id: String,
    tunnel_config: &TunnelConfig,
    interface_config: &InterfaceConfig,
    _interface_name: String,
    _all_routes: Vec<String>,
    tunnels_dir: &Path,
) -> Result<(), String> {
    let process_handle =
        start_wireguard_windows(&tunnel_id, tunnel_config, interface_config, tunnels_dir)
            .map_err(|e| format!("启动隧道失败: {}", e))?;

    {
        let mut processes = TUNNEL_PROCESSES.lock().await;
        processes.insert(tunnel_id.clone(), process_handle);
    }

    log::info!("隧道启动完成: {}", tunnel_config.name);
    Ok(())
}

// Windows: 停止隧道的清理逻辑
pub async fn cleanup_stale_tunnel(tunnel_id: &str) -> Result<(), String> {
    let sanitized_id = sanitize_identifier(tunnel_id);
    let service_name = format!("WireGuardTunnel${}", sanitized_id);
    match stop_wireguard_windows(&service_name, &sanitized_id, None) {
        Ok(_) => {
            log::info!("已尝试卸载 Windows WireGuard 服务 {}", service_name);
            Ok(())
        }
        Err(e) => {
            log::warn!("卸载 Windows WireGuard 服务失败: {}", e);
            Err(e)
        }
    }
}

// Windows 不需要 endpoint 刷新任务（官方客户端处理）
pub fn start_endpoint_refresh_task(_tunnel_id: String, _interface: String) {
    // Windows 平台由官方 WireGuard 服务处理 DNS 解析，暂不需要后台刷新任务
}
