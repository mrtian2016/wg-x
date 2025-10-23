use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::Manager;
use tokio::sync::Mutex;

use crate::commands::key_management::private_key_to_public;

// 平台特定模块
#[cfg(target_os = "macos")]
mod platform {
    pub use crate::tunnel_macos::*;
}

#[cfg(target_os = "linux")]
mod platform {
    pub use crate::tunnel_linux::*;
}

#[cfg(target_os = "windows")]
mod platform {
    pub use crate::tunnel_windows::*;
}

// 重新导出平台特定的函数
pub use platform::{cleanup_stale_tunnel, get_tunnel_status_impl, start_tunnel_platform};

// 进程包装器，用于统一管理不同类型的子进程
pub enum ProcessHandle {
    StdProcess(std::process::Child),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    PrivilegedProcess(i32), // 存储 PID,用于 macOS 和 Linux 的权限提升进程
    #[cfg(target_os = "windows")]
    WindowsService {
        service_name: String,
        interface_name: String,
        config_path: std::path::PathBuf,
    },
}

impl ProcessHandle {
    fn kill(&mut self, _tunnel_id: &str) -> Result<(), String> {
        match self {
            ProcessHandle::StdProcess(child) => {
                child.kill().map_err(|e| format!("杀死进程失败: {}", e))
            }
            #[cfg(target_os = "macos")]
            ProcessHandle::PrivilegedProcess(pid) => {
                crate::tunnel_macos::stop_wireguard_macos(*pid)
            }
            #[cfg(target_os = "linux")]
            ProcessHandle::PrivilegedProcess(pid) => {
                crate::tunnel_linux::stop_wireguard_linux(*pid, _tunnel_id)
            }
            #[cfg(target_os = "windows")]
            ProcessHandle::WindowsService {
                service_name,
                interface_name,
                config_path,
            } => crate::tunnel_windows::stop_wireguard_windows(
                service_name,
                interface_name,
                Some(config_path.as_path()),
            ),
        }
    }
}

// 全局隧道进程管理
lazy_static::lazy_static! {
    pub static ref TUNNEL_PROCESSES: Mutex<HashMap<String, ProcessHandle>> = Mutex::new(HashMap::new());
    // 保存隧道的完整配置(包含原始 endpoint 域名),用于定期更新
    pub static ref TUNNEL_CONFIGS: Mutex<HashMap<String, (String, InterfaceConfig)>> = Mutex::new(HashMap::new());
}

// 检查接口是否存在
pub fn interface_exists(name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ifconfig").arg(name).output();

        if let Ok(result) = output {
            result.status.success()
        } else {
            false
        }
    }

    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("ip")
            .args(["link", "show", name])
            .output();

        if let Ok(result) = output {
            result.status.success()
        } else {
            false
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok((_, wg_path)) = crate::tunnel_windows::locate_wireguard_tools() {
            if let Ok(output) = std::process::Command::new(&wg_path)
                .args(["show", "interfaces"])
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    return stdout
                        .split_whitespace()
                        .any(|iface| iface.eq_ignore_ascii_case(name));
                }
            }
        }
        false
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

// 生成接口名称的辅助函数
pub fn generate_interface_name(tunnel_id: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        crate::tunnel_windows::sanitize_identifier(tunnel_id)
    }

    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        let prefix = "utun";

        #[cfg(target_os = "linux")]
        let prefix = "tun";

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let prefix = "wg";

        // 使用简单的哈希算法计算 tunnel_id 的哈希值
        let mut hash: u32 = 0;
        for byte in tunnel_id.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
        }

        // 将哈希值映射到 0-99 范围内
        let number = (hash % 100) as u32;

        format!("{}{}", prefix, number)
    }
}

// 将 Base64 编码的密钥转换为十六进制编码
// WireGuard UAPI 需要十六进制编码的密钥
pub fn base64_to_hex(base64_key: &str) -> Result<String, String> {
    let bytes = BASE64
        .decode(base64_key.trim())
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    if bytes.len() != 32 {
        return Err(format!(
            "密钥长度错误: 应为32字节,实际为{}字节",
            bytes.len()
        ));
    }

    // 转换为十六进制字符串
    Ok(hex::encode(&bytes))
}

// 解析 endpoint: 如果包含域名,解析为 IP 地址
pub fn resolve_endpoint(endpoint: &str) -> Result<String, String> {
    use std::net::ToSocketAddrs;

    // 尝试解析为 SocketAddr
    match endpoint.to_socket_addrs() {
        Ok(mut addrs) => {
            if let Some(addr) = addrs.next() {
                // 返回 IP:端口 格式
                Ok(addr.to_string())
            } else {
                Err("无法解析域名".to_string())
            }
        }
        Err(e) => Err(format!("DNS 解析失败: {}", e)),
    }
}

// 解析接口状态
pub fn parse_interface_status(status: &str) -> (u64, u64, Option<i64>) {
    let mut tx_bytes = 0u64;
    let mut rx_bytes = 0u64;
    let mut last_handshake: Option<i64> = None;

    for line in status.lines() {
        let line = line.trim();

        if line.starts_with("rx_bytes=") {
            if let Some(value) = line.strip_prefix("rx_bytes=") {
                rx_bytes = value.parse().unwrap_or(0);
            }
        } else if line.starts_with("tx_bytes=") {
            if let Some(value) = line.strip_prefix("tx_bytes=") {
                tx_bytes = value.parse().unwrap_or(0);
            }
        } else if line.starts_with("last_handshake_time_sec=") {
            if let Some(value) = line.strip_prefix("last_handshake_time_sec=") {
                if let Ok(ts) = value.parse::<i64>() {
                    if ts > 0 {
                        last_handshake = Some(ts);
                    }
                }
            }
        }
    }

    (tx_bytes, rx_bytes, last_handshake)
}

// Peer 配置
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerConfig {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
    pub preshared_key: Option<String>,
}

// 接口配置
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceConfig {
    pub private_key: String,
    pub listen_port: Option<u16>,
    pub fwmark: Option<u32>,
    pub replace_peers: bool,
    pub peers: Vec<PeerConfig>,
}

// Peer 配置项 (用于存储)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TunnelPeerConfig {
    pub public_key: String,
    #[serde(default)]
    pub client_private_key: Option<String>,
    pub preshared_key: Option<String>,
    pub endpoint: Option<String>,
    pub address: Option<String>, // 客户端的 VPN IP 地址
    pub allowed_ips: String,
    pub persistent_keepalive: Option<u16>,
}

// 隧道配置(用户创建的配置)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    // 运行模式: 'server' 或 'client'
    #[serde(default)]
    pub mode: String,
    // Interface 配置
    pub private_key: String,
    pub address: String,
    pub listen_port: String, // 空字符串表示自动
    pub dns: String,
    pub mtu: String,
    // 服务端的公网 IP 或域名（仅服务端）
    #[serde(default)]
    pub server_endpoint: String,
    // 服务端允许客户端访问的网络范围（仅服务端）
    #[serde(default)]
    pub server_allowed_ips: String,
    // Peer 配置 - 支持多个 Peer
    #[serde(default)]
    pub peers: Vec<TunnelPeerConfig>,
    // 向后兼容的单个 Peer 字段
    #[serde(default)]
    pub peer_public_key: String,
    #[serde(default)]
    pub preshared_key: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub allowed_ips: String,
    #[serde(default)]
    pub persistent_keepalive: String,
    // 元数据
    pub created_at: i64,
}

// 隧道状态
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TunnelStatus {
    pub id: String,
    pub name: String,
    pub status: String, // running, stopped, connecting
    pub address: Option<String>,
    pub endpoint: Option<String>,
    pub listen_port: Option<u16>,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub last_handshake: Option<i64>,
    pub public_key: Option<String>,
    pub allowed_ips: Option<String>,
    // 运行模式和服务端地址
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub server_endpoint: String,
    #[serde(default)]
    pub server_allowed_ips: String,
    // Peer 配置列表
    #[serde(default)]
    pub peers: Vec<TunnelPeerConfig>,
}

// 启动隧道
#[tauri::command]
pub async fn start_tunnel(tunnel_id: String, app: tauri::AppHandle) -> Result<(), String> {
    // 检查隧道是否已在运行
    {
        let processes = TUNNEL_PROCESSES.lock().await;
        if processes.contains_key(&tunnel_id) {
            return Err("隧道已在运行中".to_string());
        }
    }

    // 额外检查:如果可能生成的接口已存在,说明有残留进程
    let potential_interface = generate_interface_name(&tunnel_id);
    if interface_exists(&potential_interface) {
        return Err(format!(
            "接口 {} 已存在,可能有残留进程。请先手动停止或删除该接口",
            potential_interface
        ));
    }

    // 从隧道配置目录加载配置
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config_file = app_data_dir
        .join("tunnels")
        .join(format!("{}.json", tunnel_id));

    if !config_file.exists() {
        return Err("隧道配置不存在".to_string());
    }

    let content =
        std::fs::read_to_string(&config_file).map_err(|e| format!("读取配置失败: {}", e))?;

    let tunnel_config: TunnelConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败: {}", e))?;

    // 生成接口名称
    let interface_name = generate_interface_name(&tunnel_id);

    log::debug!("interface name: {}", interface_name);

    // 构建 InterfaceConfig
    let listen_port = if tunnel_config.listen_port.is_empty() {
        None
    } else {
        tunnel_config.listen_port.parse().ok()
    };

    // 构建 Peer 配置和收集路由信息
    let mut peers = Vec::new();

    // 优先使用新的 peers 数组
    if !tunnel_config.peers.is_empty() {
        for tunnel_peer in &tunnel_config.peers {
            let allowed_ips: Vec<String> = tunnel_peer
                .allowed_ips
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            peers.push(PeerConfig {
                public_key: tunnel_peer.public_key.clone(),
                endpoint: tunnel_peer.endpoint.clone(),
                allowed_ips,
                persistent_keepalive: tunnel_peer.persistent_keepalive,
                preshared_key: tunnel_peer.preshared_key.clone(),
            });
        }
    }
    // 向后兼容:如果没有使用新格式,尝试使用旧的单个 Peer 字段
    else if !tunnel_config.peer_public_key.is_empty() {
        let keepalive = if tunnel_config.persistent_keepalive.is_empty() {
            None
        } else {
            tunnel_config.persistent_keepalive.parse().ok()
        };

        let preshared_key = if tunnel_config.preshared_key.is_empty() {
            None
        } else {
            Some(tunnel_config.preshared_key.clone())
        };

        let endpoint = if tunnel_config.endpoint.is_empty() {
            None
        } else {
            Some(tunnel_config.endpoint.clone())
        };

        let allowed_ips = if tunnel_config.allowed_ips.is_empty() {
            vec![]
        } else {
            tunnel_config
                .allowed_ips
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        };

        peers.push(PeerConfig {
            public_key: tunnel_config.peer_public_key.clone(),
            endpoint,
            allowed_ips,
            persistent_keepalive: keepalive,
            preshared_key,
        });
    }

    let interface_config = InterfaceConfig {
        private_key: tunnel_config.private_key.clone(),
        listen_port,
        fwmark: None,
        replace_peers: true,
        peers,
    };

    // 收集所有需要配置的路由
    let mut all_routes: Vec<String> = Vec::new();
    for peer in &interface_config.peers {
        for ip in &peer.allowed_ips {
            if !ip.is_empty() {
                all_routes.push(ip.clone());
            }
        }
    }

    // 获取隧道配置目录（仅 Windows 需要）
    #[cfg(target_os = "windows")]
    let tunnels_dir = config_file
        .parent()
        .ok_or_else(|| "隧道配置目录不存在".to_string())?;

    // 获取 sidecar 路径（仅 Unix 平台需要）
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let sidecar_path = app
        .path()
        .resolve("wireguard-go", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("获取 sidecar 路径失败: {}", e))?;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let sidecar_path_str = sidecar_path
        .to_str()
        .ok_or_else(|| "无法转换 sidecar 路径".to_string())?;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    log::debug!("wireguard-go 路径: {}", sidecar_path_str);

    // 调用平台特定的启动函数
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        start_tunnel_platform(
            tunnel_id,
            &tunnel_config,
            &interface_config,
            interface_name,
            all_routes,
            sidecar_path_str,
        )
        .await
    }

    #[cfg(target_os = "windows")]
    {
        start_tunnel_platform(
            tunnel_id,
            &tunnel_config,
            &interface_config,
            interface_name,
            all_routes,
            tunnels_dir,
        )
        .await
    }
}

// 停止隧道
#[tauri::command]
pub async fn stop_tunnel(tunnel_id: String) -> Result<(), String> {
    let mut processes = TUNNEL_PROCESSES.lock().await;

    if let Some(mut child) = processes.remove(&tunnel_id) {
        // 同时清理保存的配置(停止 endpoint 刷新任务)
        {
            let mut configs = TUNNEL_CONFIGS.lock().await;
            configs.remove(&tunnel_id);
            log::info!("已清理隧道配置,endpoint 刷新任务将自动停止");
        }

        child
            .kill(&tunnel_id)
            .map_err(|e| format!("停止隧道失败: {}", e))?;
        Ok(())
    } else {
        // 即使进程不在列表中,也检查接口是否存在并尝试清理
        #[cfg(target_os = "windows")]
        {
            cleanup_stale_tunnel(&tunnel_id).await?;
            return Ok(());
        }

        #[cfg(not(target_os = "windows"))]
        {
            let interface_name = generate_interface_name(&tunnel_id);
            if interface_exists(&interface_name) {
                log::info!("检测到残留接口 {},尝试清理...", interface_name);
                cleanup_stale_tunnel(&interface_name).await?;
                return Ok(());
            }
        }

        Err("隧道未运行".to_string())
    }
}

// 获取隧道列表 (已废弃,使用 get_all_tunnel_configs 替代)
// 保留此函数以保持向后兼容
#[tauri::command]
pub async fn get_tunnel_list(app: tauri::AppHandle) -> Result<Vec<TunnelStatus>, String> {
    // 直接调用新的函数
    get_all_tunnel_configs(app).await
}

// 获取隧道详情
#[tauri::command]
pub async fn get_tunnel_details(
    tunnel_id: String,
    app: tauri::AppHandle,
) -> Result<TunnelStatus, String> {
    // 获取应用数据目录
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    // 从隧道配置目录加载配置
    let config_file = app_data_dir
        .join("tunnels")
        .join(format!("{}.json", tunnel_id));

    if !config_file.exists() {
        return Err("隧道配置不存在".to_string());
    }

    let content =
        std::fs::read_to_string(&config_file).map_err(|e| format!("读取配置失败: {}", e))?;

    let tunnel_config: TunnelConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败: {}", e))?;

    // 检查隧道是否在运行
    let is_in_process_list = {
        let processes = TUNNEL_PROCESSES.lock().await;
        processes.contains_key(&tunnel_id)
    };

    // 生成接口名称并检查是否存在
    let interface_name = generate_interface_name(&tunnel_id);
    let interface_exists = interface_exists(&interface_name);
    let is_running = is_in_process_list || interface_exists;

    // 如果运行中,获取实时状态
    let (tx_bytes, rx_bytes, last_handshake) = if is_running {
        get_tunnel_status_impl(&tunnel_id, &interface_name).await
    } else {
        (0, 0, None)
    };

    // 从 peers 数组或旧格式字段中提取 endpoint 和 allowed_ips
    let (endpoint, allowed_ips) = if !tunnel_config.peers.is_empty() {
        let first_peer = &tunnel_config.peers[0];
        (
            first_peer.endpoint.clone(),
            Some(first_peer.allowed_ips.clone()),
        )
    } else {
        (
            if tunnel_config.endpoint.is_empty() {
                None
            } else {
                Some(tunnel_config.endpoint.clone())
            },
            if tunnel_config.allowed_ips.is_empty() {
                None
            } else {
                Some(tunnel_config.allowed_ips.clone())
            },
        )
    };

    // 计算公钥 (如果有私钥的话)
    let public_key = if !tunnel_config.private_key.is_empty() {
        private_key_to_public(tunnel_config.private_key.clone()).ok()
    } else {
        None
    };

    Ok(TunnelStatus {
        id: tunnel_id,
        name: tunnel_config.name.clone(),
        status: if is_running {
            "running".to_string()
        } else {
            "stopped".to_string()
        },
        address: Some(tunnel_config.address.clone()),
        endpoint,
        listen_port: tunnel_config.listen_port.parse().ok(),
        tx_bytes,
        rx_bytes,
        last_handshake,
        public_key,
        allowed_ips,
        mode: tunnel_config.mode.clone(),
        server_endpoint: tunnel_config.server_endpoint.clone(),
        server_allowed_ips: tunnel_config.server_allowed_ips.clone(),
        peers: tunnel_config.peers.clone(),
    })
}

// ========== 新的隧道配置管理命令 ==========

// 保存隧道配置
#[tauri::command]
pub async fn save_tunnel_config(
    app: tauri::AppHandle,
    config: TunnelConfig,
) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let tunnels_dir = app_data_dir.join("tunnels");
    std::fs::create_dir_all(&tunnels_dir).map_err(|e| format!("创建隧道目录失败: {}", e))?;

    let file_path = tunnels_dir.join(format!("{}.json", config.id));
    let json =
        serde_json::to_string_pretty(&config).map_err(|e| format!("序列化隧道配置失败: {}", e))?;

    std::fs::write(&file_path, json).map_err(|e| format!("保存隧道配置失败: {}", e))?;

    Ok(())
}

// 获取隧道完整配置(用于编辑)
#[tauri::command]
pub async fn get_tunnel_config(
    app: tauri::AppHandle,
    tunnel_id: String,
) -> Result<TunnelConfig, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config_file = app_data_dir
        .join("tunnels")
        .join(format!("{}.json", tunnel_id));

    if !config_file.exists() {
        return Err("隧道配置不存在".to_string());
    }

    let content =
        std::fs::read_to_string(&config_file).map_err(|e| format!("读取配置失败: {}", e))?;

    let tunnel_config: TunnelConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败: {}", e))?;

    Ok(tunnel_config)
}

// 删除隧道配置
#[tauri::command]
pub async fn delete_tunnel_config(app: tauri::AppHandle, tunnel_id: String) -> Result<(), String> {
    // 确保隧道未运行
    {
        let processes = TUNNEL_PROCESSES.lock().await;
        if processes.contains_key(&tunnel_id) {
            return Err("请先停止隧道再删除配置".to_string());
        }
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let file_path = app_data_dir
        .join("tunnels")
        .join(format!("{}.json", tunnel_id));

    if file_path.exists() {
        std::fs::remove_file(&file_path).map_err(|e| format!("删除隧道配置失败: {}", e))?;
    }

    Ok(())
}

// 获取所有隧道配置列表 (包括运行和停止的)
#[tauri::command]
pub async fn get_all_tunnel_configs(app: tauri::AppHandle) -> Result<Vec<TunnelStatus>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let tunnels_dir = app_data_dir.join("tunnels");

    log::debug!("检查隧道目录: {:?}", tunnels_dir);

    if !tunnels_dir.exists() {
        log::debug!("隧道目录不存在");
        return Ok(Vec::new());
    }

    let mut tunnels = Vec::new();

    // 获取运行中的隧道 ID 列表
    let running_tunnels: Vec<String> = {
        let processes = TUNNEL_PROCESSES.lock().await;
        processes.keys().cloned().collect()
    };

    // 读取所有隧道配置
    let entries =
        std::fs::read_dir(&tunnels_dir).map_err(|e| format!("读取隧道目录失败: {}", e))?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    match serde_json::from_str::<TunnelConfig>(&content) {
                        Ok(tunnel_config) => {
                            log::debug!(
                                "解析配置成功: id={}, name={}",
                                tunnel_config.id,
                                tunnel_config.name
                            );
                            let is_in_process_list = running_tunnels.contains(&tunnel_config.id);

                            // 生成接口名称
                            let interface_name = generate_interface_name(&tunnel_config.id);
                            let interface_exists = interface_exists(&interface_name);

                            // 判断实际运行状态
                            let is_running = is_in_process_list || interface_exists;

                            let (tx_bytes, rx_bytes, last_handshake) = if is_running {
                                get_tunnel_status_impl(&tunnel_config.id, &interface_name).await
                            } else {
                                (0, 0, None)
                            };

                            // 从 peers 数组或旧格式字段中提取 endpoint 和 allowed_ips
                            let (endpoint, allowed_ips) = if !tunnel_config.peers.is_empty() {
                                // 使用新格式: peers 数组 (取第一个 peer 的信息用于显示)
                                let first_peer = &tunnel_config.peers[0];
                                (
                                    first_peer.endpoint.clone(),
                                    Some(first_peer.allowed_ips.clone()),
                                )
                            } else {
                                // 向后兼容: 使用旧格式字段
                                (
                                    if tunnel_config.endpoint.is_empty() {
                                        None
                                    } else {
                                        Some(tunnel_config.endpoint.clone())
                                    },
                                    if tunnel_config.allowed_ips.is_empty() {
                                        None
                                    } else {
                                        Some(tunnel_config.allowed_ips.clone())
                                    },
                                )
                            };

                            let tunnel_status = TunnelStatus {
                                id: tunnel_config.id.clone(),
                                name: tunnel_config.name.clone(),
                                status: if is_running {
                                    "running".to_string()
                                } else {
                                    "stopped".to_string()
                                },
                                address: Some(tunnel_config.address.clone()),
                                endpoint,
                                listen_port: tunnel_config.listen_port.parse().ok(),
                                tx_bytes,
                                rx_bytes,
                                last_handshake,
                                public_key: None, // 不暴露公钥
                                allowed_ips,
                                mode: tunnel_config.mode.clone(),
                                server_endpoint: tunnel_config.server_endpoint.clone(),
                                server_allowed_ips: tunnel_config.server_allowed_ips.clone(),
                                peers: tunnel_config.peers.clone(),
                            };

                            tunnels.push(tunnel_status);
                        }
                        Err(e) => {
                            log::warn!("解析配置失败: {}", e);
                        }
                    }
                }
            }
        }
    }

    // 按创建时间降序排序
    tunnels.sort_by(|a, b| b.id.cmp(&a.id));

    Ok(tunnels)
}
