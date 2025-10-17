use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tauri::Manager;
#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;
use tauri::{WebviewUrl, WebviewWindowBuilder};
use x25519_dalek::x25519;

// WebDAV 同步模块
mod webdav;
mod sync;

use sync::{SyncManager, SyncResult};
use webdav::{WebDavConfig, LastSyncInfo};
// X25519 基点 (标准值)
const X25519_BASEPOINT: [u8; 32] = [
    9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

// 数据结构定义
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyPair {
    pub private_key: String,
    pub public_key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WgConfig {
    // Interface 配置
    pub interface_name: String,
    pub private_key: String,
    pub address: String,
    pub listen_port: Option<String>,
    pub dns: Option<String>,

    // Peer 配置
    pub peer_public_key: String,
    pub preshared_key: Option<String>,
    pub endpoint: String,
    pub allowed_ips: String,
    pub persistent_keepalive: Option<String>,

    // 爱快配置
    pub ikuai_id: u32,
    pub ikuai_interface: String,
    pub ikuai_comment: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EnvConfig {
    pub peer_public_key: Option<String>,
    pub endpoint: Option<String>,
    pub preshared_key: Option<String>,
    pub allowed_ips: Option<String>,
    pub interface_name: Option<String>,
    pub ikuai_interface: Option<String>,
    pub listen_port: Option<String>,
    pub dns_server: Option<String>,
    pub keepalive: Option<String>,
}

// WireGuard 密钥生成
#[tauri::command]
fn generate_keypair() -> Result<KeyPair, String> {
    // 生成 32 字节随机私钥
    let mut private_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut private_bytes);

    // WireGuard/X25519 私钥 clamping
    clamp_private_key(&mut private_bytes);

    // 先编码私钥
    let private_key = BASE64.encode(&private_bytes);

    // 使用 x25519 函数从私钥和基点计算公钥
    let public_bytes = x25519(private_bytes, X25519_BASEPOINT);
    let public_key = BASE64.encode(&public_bytes);

    Ok(KeyPair {
        private_key,
        public_key,
    })
}

// X25519/WireGuard 私钥 clamping
fn clamp_private_key(key: &mut [u8; 32]) {
    key[0] &= 248; // 清除最低 3 位
    key[31] &= 127; // 清除最高位
    key[31] |= 64; // 设置次高位
}

// 生成预共享密钥
#[tauri::command]
fn generate_preshared_key() -> Result<String, String> {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    Ok(BASE64.encode(key))
}

// 从私钥计算公钥
#[tauri::command]
fn private_key_to_public(private_key: String) -> Result<String, String> {
    let bytes = BASE64
        .decode(private_key.trim())
        .map_err(|e| format!("无效的私钥格式: {}", e))?;

    if bytes.len() != 32 {
        return Err("私钥长度必须为32字节".to_string());
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);

    // 确保私钥经过 clamping (如果用户输入的私钥未经处理)
    clamp_private_key(&mut key_bytes);

    // 使用 x25519 函数计算公钥
    let public_bytes = x25519(key_bytes, X25519_BASEPOINT);

    Ok(BASE64.encode(&public_bytes))
}

// 加载环境变量配置
#[tauri::command]
fn load_env_config(work_dir: String) -> Result<EnvConfig, String> {
    let env_path = Path::new(&work_dir).join("wg.env");

    if !env_path.exists() {
        return Ok(EnvConfig {
            peer_public_key: None,
            endpoint: None,
            preshared_key: None,
            allowed_ips: None,
            interface_name: None,
            ikuai_interface: None,
            listen_port: None,
            dns_server: None,
            keepalive: None,
        });
    }

    let content = fs::read_to_string(&env_path).map_err(|e| format!("读取 wg.env 失败: {}", e))?;

    let mut config = EnvConfig {
        peer_public_key: None,
        endpoint: None,
        preshared_key: None,
        allowed_ips: None,
        interface_name: None,
        ikuai_interface: None,
        listen_port: None,
        dns_server: None,
        keepalive: None,
    };

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match key {
                "WG_PEER_PUBLIC_KEY" => config.peer_public_key = Some(value.to_string()),
                "WG_ENDPOINT" => config.endpoint = Some(value.to_string()),
                "WG_PRESHARED_KEY" => config.preshared_key = Some(value.to_string()),
                "WG_ALLOWED_IPS" => config.allowed_ips = Some(value.to_string()),
                "WG_INTERFACE_NAME" => config.interface_name = Some(value.to_string()),
                "WG_IKUAI_INTERFACE" => config.ikuai_interface = Some(value.to_string()),
                "WG_LISTEN_PORT" => config.listen_port = Some(value.to_string()),
                "WG_DNS_SERVER" => config.dns_server = Some(value.to_string()),
                "WG_KEEPALIVE" => config.keepalive = Some(value.to_string()),
                _ => {}
            }
        }
    }

    Ok(config)
}

// 获取下一个可用的 Peer ID（从持久化配置读取）
#[tauri::command]
fn get_next_peer_id(app: tauri::AppHandle) -> Result<u32, String> {
    let config = load_persistent_config(app)?;

    // 如果 next_peer_id 为 0（默认值），返回 1
    if config.next_peer_id == 0 {
        Ok(1)
    } else {
        Ok(config.next_peer_id)
    }
}

// 生成标准 WireGuard 配置文件（仅返回内容，不保存文件）
#[tauri::command]
fn generate_wg_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
    let mut content = format!(
        "# 本地公钥 (提供给对端): {}\n\n[Interface]\nPrivateKey = {}\nAddress = {}\n",
        compute_public_key(&config.private_key)?,
        config.private_key,
        config.address
    );

    if let Some(port) = &config.listen_port {
        if !port.is_empty() {
            content.push_str(&format!("ListenPort = {}\n", port));
        }
    }

    if let Some(dns) = &config.dns {
        if !dns.is_empty() {
            content.push_str(&format!("DNS = {}\n", dns));
        }
    }

    content.push_str(&format!(
        "\n[Peer]\nPublicKey = {}\n",
        config.peer_public_key
    ));

    if let Some(psk) = &config.preshared_key {
        if !psk.is_empty() {
            content.push_str(&format!("PresharedKey = {}\n", psk));
        }
    }

    content.push_str(&format!(
        "Endpoint = {}\nAllowedIPs = {}\n",
        config.endpoint, config.allowed_ips
    ));

    if let Some(keepalive) = &config.persistent_keepalive {
        if !keepalive.is_empty() {
            content.push_str(&format!("PersistentKeepalive = {}\n", keepalive));
        }
    }

    Ok(content)
}

// 生成爱快 Peer 配置（仅返回内容，不保存文件）
#[tauri::command]
fn generate_ikuai_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
    let public_key = compute_public_key(&config.private_key)?;

    let psk = config.preshared_key.unwrap_or_default();
    let keepalive = config
        .persistent_keepalive
        .unwrap_or_else(|| "25".to_string());

    let ikuai_line = format!(
        "id={} enabled=yes comment={} interface={} peer_publickey={} presharedkey={} allowips={} endpoint= endpoint_port= keepalive={}",
        config.ikuai_id,
        config.ikuai_comment,
        config.ikuai_interface,
        public_key,
        psk,
        config.address,
        keepalive
    );

    Ok(ikuai_line)
}

// 生成 Surge 配置（仅返回内容，不保存文件）
#[tauri::command]
fn generate_surge_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
    // 提取 IP 地址（去掉 CIDR 前缀）
    let self_ip = config.address.split('/').next().unwrap_or(&config.address);

    // Section 名称使用备注名称
    let section_name = config.interface_name.replace(" ", "");

    let mut surge_config = String::new();

    // [Proxy] 部分
    surge_config.push_str(&format!("[Proxy]\n"));
    surge_config.push_str(&format!(
        "wireguard-{} = wireguard, section-name = {}\n\n",
        section_name, section_name
    ));

    // [WireGuard Section] 部分
    surge_config.push_str(&format!("[WireGuard {}]\n", section_name));
    surge_config.push_str(&format!("private-key = {}\n", config.private_key));
    surge_config.push_str(&format!("self-ip = {}\n", self_ip));

    // DNS 配置（可选）
    // if let Some(dns) = &config.dns {
    //     if !dns.is_empty() {
    //         surge_config.push_str(&format!("dns-server = {}\n", dns));
    //     }
    // }

    // MTU（可选，Surge 推荐 1280）
    surge_config.push_str("mtu = 1280\n");

    // Peer 配置
    let mut peer_config = format!("peer = (public-key = {}", config.peer_public_key);

    // AllowedIPs
    peer_config.push_str(&format!(", allowed-ips = \"{}\"", config.allowed_ips));

    // Endpoint
    peer_config.push_str(&format!(", endpoint = {}", config.endpoint));

    // PresharedKey（可选）
    if let Some(psk) = &config.preshared_key {
        if !psk.is_empty() {
            peer_config.push_str(&format!(", preshared-key = {}", psk));
        }
    }

    // Keepalive（可选）
    if let Some(keepalive) = &config.persistent_keepalive {
        if !keepalive.is_empty() {
            peer_config.push_str(&format!(", keepalive = {}", keepalive));
        }
    }

    peer_config.push_str(")\n");
    surge_config.push_str(&peer_config);

    Ok(surge_config)
}

// 生成 MikroTik RouterOS 配置（仅返回内容，不保存文件）
#[tauri::command]
fn generate_mikrotik_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
    let public_key = compute_public_key(&config.private_key)?;

    // 提取 IP 地址（去掉 CIDR 前缀，保留子网掩码）
    let allowed_address = &config.address;

    // 构建 MikroTik 命令
    let mut command = format!(
        "/interface/wireguard/peers/add \\\n  interface={} \\\n  public-key=\"{}\" \\\n  allowed-address={} \\\n  comment=\"{}\"",
        config.ikuai_interface,
        public_key,
        allowed_address,
        config.ikuai_comment
    );

    // 添加预共享密钥（如果有）
    if let Some(psk) = &config.preshared_key {
        if !psk.is_empty() {
            command.push_str(&format!(" \\\n  preshared-key=\"{}\"", psk));
        }
    }

    Ok(command)
}

// 生成 OpenWrt UCI 配置（仅返回内容，不保存文件）
#[tauri::command]
fn generate_openwrt_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
    let public_key = compute_public_key(&config.private_key)?;

    // 从接口名称获取 UCI section 名称（例如：wg0 -> wireguard_wg0）
    let section_name = format!("wireguard_{}", config.interface_name);

    let mut commands = String::new();

    // 添加 Peer
    commands.push_str(&format!("uci add network {}\n", section_name));
    commands.push_str(&format!(
        "uci set network.@{}[-1].public_key='{}'\n",
        section_name, public_key
    ));

    // 添加预共享密钥（如果有）
    if let Some(psk) = &config.preshared_key {
        if !psk.is_empty() {
            commands.push_str(&format!(
                "uci set network.@{}[-1].preshared_key='{}'\n",
                section_name, psk
            ));
        }
    }

    // 添加 allowed_ips
    commands.push_str(&format!(
        "uci set network.@{}[-1].allowed_ips='{}'\n",
        section_name, config.address
    ));

    // 添加 persistent_keepalive（如果有）
    if let Some(keepalive) = &config.persistent_keepalive {
        if !keepalive.is_empty() {
            commands.push_str(&format!(
                "uci set network.@{}[-1].persistent_keepalive='{}'\n",
                section_name, keepalive
            ));
        }
    }

    // 添加备注
    commands.push_str(&format!(
        "uci set network.@{}[-1].description='{}'\n",
        section_name, config.ikuai_comment
    ));

    // 提交配置并重启接口
    commands.push_str("# 提交配置\n");
    commands.push_str("uci commit network\n");
    commands.push_str(&format!("ifup {}", config.interface_name));

    Ok(commands)
}

// 辅助函数：从私钥计算公钥
fn compute_public_key(private_key: &str) -> Result<String, String> {
    let bytes = BASE64
        .decode(private_key.trim())
        .map_err(|e| format!("无效的私钥格式: {}", e))?;

    if bytes.len() != 32 {
        return Err("私钥长度必须为32字节".to_string());
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);

    // 确保私钥经过 clamping
    clamp_private_key(&mut key_bytes);

    // 使用 x25519 函数计算公钥
    let public_bytes = x25519(key_bytes, X25519_BASEPOINT);

    Ok(BASE64.encode(&public_bytes))
}

// 服务端配置结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub id: String,                   // 唯一ID
    pub name: String,                 // 服务端名称
    pub peer_public_key: String,      // 服务端公钥
    pub preshared_key: String,        // 预共享密钥
    pub endpoint: String,             // Endpoint地址
    pub allowed_ips: String,          // AllowedIPs
    pub persistent_keepalive: String, // Keepalive
    pub ikuai_interface: String,      // 爱快接口名称
    pub next_peer_id: u32,            // 该服务端的Peer ID计数器
    pub created_at: i64,              // 创建时间
}

// 持久化配置结构（保留用于数据迁移）
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PersistentConfig {
    // 对端配置
    pub peer_public_key: String,
    pub preshared_key: String,
    pub endpoint: String,
    pub allowed_ips: String,
    pub persistent_keepalive: String,

    // 爱快配置
    pub ikuai_interface: String,

    // Peer ID 计数器
    pub next_peer_id: u32,
}

// 历史记录条目
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HistoryEntry {
    pub id: String,                      // 唯一ID (时间戳)
    pub timestamp: i64,                  // 创建时间戳
    pub interface_name: String,          // 接口名称
    pub ikuai_comment: String,           // 备注名称
    pub ikuai_id: u32,                   // Peer ID
    pub address: String,                 // IP 地址
    pub wg_config: String,               // WireGuard 配置内容
    pub ikuai_config: String,            // 爱快配置内容
    pub surge_config: Option<String>,    // Surge 配置内容（可选，兼容旧数据）
    pub mikrotik_config: Option<String>, // MikroTik 配置内容（可选，兼容旧数据）
    pub openwrt_config: Option<String>,  // OpenWrt 配置内容（可选，兼容旧数据）
    pub public_key: String,              // 公钥
    pub server_id: String,               // 关联的服务端ID
    pub server_name: String,             // 服务端名称（冗余存储）
}

// 历史记录列表项（用于列表显示，不包含完整配置内容）
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HistoryListItem {
    pub id: String,
    pub timestamp: i64,
    pub interface_name: String,
    pub ikuai_comment: String,
    pub ikuai_id: u32,
    pub address: String,
    pub public_key: String,
    pub server_id: String,
    pub server_name: String,
}

// 保存持久化配置（使用 Tauri 应用数据目录）
#[tauri::command]
fn save_persistent_config(app: tauri::AppHandle, config: PersistentConfig) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    // 确保目录存在
    fs::create_dir_all(&app_data_dir).map_err(|e| format!("创建应用数据目录失败: {}", e))?;

    let config_path = app_data_dir.join("config.json");
    let json =
        serde_json::to_string_pretty(&config).map_err(|e| format!("序列化配置失败: {}", e))?;

    fs::write(&config_path, json).map_err(|e| format!("保存配置失败: {}", e))?;

    Ok(())
}

// 加载持久化配置（从 Tauri 应用数据目录）
#[tauri::command]
fn load_persistent_config(app: tauri::AppHandle) -> Result<PersistentConfig, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config_path = app_data_dir.join("config.json");

    if !config_path.exists() {
        return Ok(PersistentConfig::default());
    }

    let content = fs::read_to_string(&config_path).map_err(|e| format!("读取配置失败: {}", e))?;

    let config: PersistentConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败: {}", e))?;

    Ok(config)
}

// 生成二维码（返回 base64 PNG）
#[tauri::command]
fn generate_qrcode(content: String) -> Result<String, String> {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let code = QrCode::new(content.as_bytes()).map_err(|e| format!("生成二维码失败: {}", e))?;

    // 生成 SVG 格式
    let svg = code.render::<svg::Color>().min_dimensions(200, 200).build();

    // 返回 data URL 格式
    let data_url = format!(
        "data:image/svg+xml;base64,{}",
        BASE64.encode(svg.as_bytes())
    );

    Ok(data_url)
}

// 保存配置文件到指定路径
#[tauri::command]
fn save_config_to_path(content: String, file_path: String) -> Result<(), String> {
    fs::write(&file_path, content).map_err(|e| format!("保存文件失败: {}", e))?;
    Ok(())
}

// 保存配置到历史记录
#[tauri::command]
fn save_to_history(app: tauri::AppHandle, entry: HistoryEntry) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let history_dir = app_data_dir.join("history");
    fs::create_dir_all(&history_dir).map_err(|e| format!("创建历史目录失败: {}", e))?;

    let file_path = history_dir.join(format!("{}.json", entry.id));
    let json =
        serde_json::to_string_pretty(&entry).map_err(|e| format!("序列化历史记录失败: {}", e))?;

    fs::write(&file_path, json).map_err(|e| format!("保存历史记录失败: {}", e))?;

    Ok(())
}

// 获取历史记录列表
#[tauri::command]
fn get_history_list(app: tauri::AppHandle) -> Result<Vec<HistoryListItem>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let history_dir = app_data_dir.join("history");

    if !history_dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();

    let entries = fs::read_dir(&history_dir).map_err(|e| format!("读取历史目录失败: {}", e))?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(history_entry) = serde_json::from_str::<HistoryEntry>(&content) {
                        items.push(HistoryListItem {
                            id: history_entry.id,
                            timestamp: history_entry.timestamp,
                            interface_name: history_entry.interface_name,
                            ikuai_comment: history_entry.ikuai_comment,
                            ikuai_id: history_entry.ikuai_id,
                            address: history_entry.address,
                            public_key: history_entry.public_key,
                            server_id: history_entry.server_id,
                            server_name: history_entry.server_name,
                        });
                    }
                }
            }
        }
    }

    // 按时间戳降序排序（最新的在前面）
    items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(items)
}

// 获取单个历史记录详情
#[tauri::command]
fn get_history_detail(app: tauri::AppHandle, id: String) -> Result<HistoryEntry, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let file_path = app_data_dir.join("history").join(format!("{}.json", id));

    if !file_path.exists() {
        return Err("历史记录不存在".to_string());
    }

    let content = fs::read_to_string(&file_path).map_err(|e| format!("读取历史记录失败: {}", e))?;

    let entry: HistoryEntry =
        serde_json::from_str(&content).map_err(|e| format!("解析历史记录失败: {}", e))?;

    Ok(entry)
}

// 删除历史记录
#[tauri::command]
async fn delete_history(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let filename = format!("{}.json", id);
    let file_path = app_data_dir.join("history").join(&filename);

    if file_path.exists() {
        fs::remove_file(&file_path).map_err(|e| format!("删除历史记录失败: {}", e))?;

        // 记录删除操作，以便同步时删除远程文件
        let manager = SyncManager::new(app_data_dir);
        if let Err(e) = manager.record_deletion("history", &filename).await {
            eprintln!("记录删除操作失败: {}", e);
        }
    }

    Ok(())
}

// 清空所有历史记录
#[tauri::command]
async fn clear_all_history(app: tauri::AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let history_dir = app_data_dir.join("history");

    if history_dir.exists() {
        // 先读取所有文件名，记录删除操作
        let manager = SyncManager::new(app_data_dir.clone());

        if let Ok(entries) = fs::read_dir(&history_dir) {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.ends_with(".json") {
                        // 记录每个文件的删除
                        if let Err(e) = manager.record_deletion("history", filename).await {
                            eprintln!("记录删除操作失败: {}", e);
                        }
                    }
                }
            }
        }

        // 删除目录
        fs::remove_dir_all(&history_dir).map_err(|e| format!("清空历史记录失败: {}", e))?;
    }

    Ok(())
}

// 清空缓存配置（删除 config.json）
#[tauri::command]
fn clear_cached_config(app: tauri::AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config_path = app_data_dir.join("config.json");

    if config_path.exists() {
        fs::remove_file(&config_path).map_err(|e| format!("删除缓存配置失败: {}", e))?;
    }

    Ok(())
}

// ========== 数据迁移命令 ==========

// 迁移旧配置到新的服务端结构
#[tauri::command]
fn migrate_old_config_to_server(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let old_config_path = app_data_dir.join("config.json");

    // 检查旧配置是否存在
    if !old_config_path.exists() {
        return Ok(None);
    }

    // 读取旧配置
    let content =
        fs::read_to_string(&old_config_path).map_err(|e| format!("读取旧配置失败: {}", e))?;

    let old_config: PersistentConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析旧配置失败: {}", e))?;

    // 检查是否有有效的配置数据
    if old_config.peer_public_key.is_empty() || old_config.endpoint.is_empty() {
        // 旧配置为空，不需要迁移
        return Ok(None);
    }

    // 创建新的服务端配置
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let server_id = format!("migrated_{}", timestamp);
    let server_config = ServerConfig {
        id: server_id.clone(),
        name: "默认服务端（迁移）".to_string(),
        peer_public_key: old_config.peer_public_key,
        preshared_key: old_config.preshared_key,
        endpoint: old_config.endpoint,
        allowed_ips: old_config.allowed_ips,
        persistent_keepalive: old_config.persistent_keepalive,
        ikuai_interface: old_config.ikuai_interface,
        next_peer_id: old_config.next_peer_id,
        created_at: timestamp,
    };

    // 保存新的服务端配置
    save_server_config(app.clone(), server_config)?;

    // 删除旧配置文件（可选，也可以重命名为 .bak）
    fs::rename(&old_config_path, app_data_dir.join("config.json.bak"))
        .map_err(|e| format!("备份旧配置失败: {}", e))?;

    Ok(Some(server_id))
}

// ========== 服务端管理命令 ==========

// 保存服务端配置（新建或更新）
#[tauri::command]
fn save_server_config(app: tauri::AppHandle, config: ServerConfig) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let servers_dir = app_data_dir.join("servers");
    fs::create_dir_all(&servers_dir).map_err(|e| format!("创建服务端目录失败: {}", e))?;

    let file_path = servers_dir.join(format!("{}.json", config.id));
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("序列化服务端配置失败: {}", e))?;

    fs::write(&file_path, json).map_err(|e| format!("保存服务端配置失败: {}", e))?;

    Ok(())
}

// 获取所有服务端列表
#[tauri::command]
fn get_server_list(app: tauri::AppHandle) -> Result<Vec<ServerConfig>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let servers_dir = app_data_dir.join("servers");

    if !servers_dir.exists() {
        return Ok(Vec::new());
    }

    let mut servers = Vec::new();

    let entries = fs::read_dir(&servers_dir).map_err(|e| format!("读取服务端目录失败: {}", e))?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(server) = serde_json::from_str::<ServerConfig>(&content) {
                        servers.push(server);
                    }
                }
            }
        }
    }

    // 按创建时间降序排序
    servers.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(servers)
}

// 获取单个服务端详情
#[tauri::command]
fn get_server_detail(app: tauri::AppHandle, id: String) -> Result<ServerConfig, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let file_path = app_data_dir.join("servers").join(format!("{}.json", id));

    if !file_path.exists() {
        return Err("服务端配置不存在".to_string());
    }

    let content =
        fs::read_to_string(&file_path).map_err(|e| format!("读取服务端配置失败: {}", e))?;

    let server: ServerConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析服务端配置失败: {}", e))?;

    Ok(server)
}

// 删除服务端配置
#[tauri::command]
async fn delete_server(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let filename = format!("{}.json", id);
    let file_path = app_data_dir.join("servers").join(&filename);

    if file_path.exists() {
        fs::remove_file(&file_path).map_err(|e| format!("删除服务端配置失败: {}", e))?;

        // 记录删除操作，以便同步时删除远程文件
        let manager = SyncManager::new(app_data_dir);
        if let Err(e) = manager.record_deletion("servers", &filename).await {
            eprintln!("记录删除操作失败: {}", e);
        }
    }

    Ok(())
}

// 清空所有服务端配置
#[tauri::command]
async fn clear_all_servers(app: tauri::AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let servers_dir = app_data_dir.join("servers");

    if servers_dir.exists() {
        // 先读取所有文件名,记录删除操作
        let manager = SyncManager::new(app_data_dir.clone());

        if let Ok(entries) = fs::read_dir(&servers_dir) {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.ends_with(".json") {
                        // 记录每个文件的删除
                        if let Err(e) = manager.record_deletion("servers", filename).await {
                            eprintln!("记录删除操作失败: {}", e);
                        }
                    }
                }
            }
        }

        // 删除目录
        fs::remove_dir_all(&servers_dir).map_err(|e| format!("清空服务端配置失败: {}", e))?;
    }

    Ok(())
}

// 获取指定服务端的下一个 Peer ID
#[tauri::command]
fn get_next_peer_id_for_server(app: tauri::AppHandle, server_id: String) -> Result<u32, String> {
    let server = get_server_detail(app, server_id)?;

    if server.next_peer_id == 0 {
        Ok(1)
    } else {
        Ok(server.next_peer_id)
    }
}

// 更新服务端的 Peer ID 计数器
#[tauri::command]
fn update_server_peer_id(
    app: tauri::AppHandle,
    server_id: String,
    next_peer_id: u32,
) -> Result<(), String> {
    let mut server = get_server_detail(app.clone(), server_id)?;
    server.next_peer_id = next_peer_id;
    save_server_config(app, server)?;
    Ok(())
}

// 按服务端获取历史记录列表
#[tauri::command]
fn get_history_list_by_server(
    app: tauri::AppHandle,
    server_id: String,
) -> Result<Vec<HistoryListItem>, String> {
    let all_history = get_history_list(app)?;

    let filtered: Vec<HistoryListItem> = all_history
        .into_iter()
        .filter(|item| item.server_id == server_id)
        .collect();

    Ok(filtered)
}

// ========== WebDAV 同步命令 ==========

// 保存 WebDAV 配置
#[tauri::command]
fn save_webdav_config(app: tauri::AppHandle, config: WebDavConfig) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    fs::create_dir_all(&app_data_dir).map_err(|e| format!("创建应用数据目录失败: {}", e))?;

    let config_path = app_data_dir.join("webdav.json");
    let json =
        serde_json::to_string_pretty(&config).map_err(|e| format!("序列化配置失败: {}", e))?;

    fs::write(&config_path, json).map_err(|e| format!("保存配置失败: {}", e))?;

    Ok(())
}

// 加载 WebDAV 配置
#[tauri::command]
fn load_webdav_config(app: tauri::AppHandle) -> Result<WebDavConfig, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config_path = app_data_dir.join("webdav.json");

    if !config_path.exists() {
        return Ok(WebDavConfig::default());
    }

    let content = fs::read_to_string(&config_path).map_err(|e| format!("读取配置失败: {}", e))?;

    let config: WebDavConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败: {}", e))?;

    Ok(config)
}

// 测试 WebDAV 连接
#[tauri::command]
async fn test_webdav_connection(config: WebDavConfig) -> Result<(), String> {
    let client = webdav::WebDavClient::new(config)?;
    client.test_connection().await
}

// 手动触发同步到远程
#[tauri::command]
async fn sync_to_webdav(app: tauri::AppHandle) -> Result<SyncResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config = load_webdav_config(app)?;

    if !config.enabled {
        return Err("WebDAV 同步未启用".to_string());
    }

    let manager = SyncManager::new(app_data_dir);
    manager.init_client(config).await?;
    manager.sync_to_remote().await
}

// 手动触发从远程同步
#[tauri::command]
async fn sync_from_webdav(app: tauri::AppHandle) -> Result<SyncResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config = load_webdav_config(app)?;

    if !config.enabled {
        return Err("WebDAV 同步未启用".to_string());
    }

    let manager = SyncManager::new(app_data_dir);
    manager.init_client(config).await?;
    manager.sync_from_remote().await
}

// 双向智能同步
#[tauri::command]
async fn sync_bidirectional_webdav(app: tauri::AppHandle) -> Result<SyncResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config = load_webdav_config(app.clone())?;

    if !config.enabled {
        return Err("WebDAV 同步未启用".to_string());
    }

    let manager = SyncManager::new(app_data_dir);
    manager.init_client(config).await?;
    let result = manager.sync_bidirectional().await?;

    // 保存同步信息
    let sync_info = LastSyncInfo {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        sync_type: "bidirectional".to_string(),
        servers_uploaded: result.servers_uploaded,
        servers_downloaded: result.servers_downloaded,
        history_uploaded: result.history_uploaded,
        history_downloaded: result.history_downloaded,
    };

    if let Err(e) = save_last_sync_info(app, sync_info) {
        eprintln!("保存同步信息失败: {}", e);
    }

    Ok(result)
}

// 保存最后同步信息
#[tauri::command]
fn save_last_sync_info(app: tauri::AppHandle, info: LastSyncInfo) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    fs::create_dir_all(&app_data_dir).map_err(|e| format!("创建应用数据目录失败: {}", e))?;

    let sync_info_path = app_data_dir.join("last_sync.json");
    let json = serde_json::to_string_pretty(&info)
        .map_err(|e| format!("序列化同步信息失败: {}", e))?;

    fs::write(&sync_info_path, json).map_err(|e| format!("保存同步信息失败: {}", e))?;

    Ok(())
}

// 读取最后同步信息
#[tauri::command]
fn load_last_sync_info(app: tauri::AppHandle) -> Result<Option<LastSyncInfo>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let sync_info_path = app_data_dir.join("last_sync.json");

    if !sync_info_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&sync_info_path)
        .map_err(|e| format!("读取同步信息失败: {}", e))?;

    let info: LastSyncInfo = serde_json::from_str(&content)
        .map_err(|e| format!("解析同步信息失败: {}", e))?;

    Ok(Some(info))
}

// 导出所有配置为 ZIP 压缩包
#[tauri::command]
fn export_all_configs_zip(app: tauri::AppHandle, zip_path: String) -> Result<(), String> {
    use std::io::Write;
    use zip::write::FileOptions;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let history_dir = app_data_dir.join("history");

    if !history_dir.exists() {
        return Err("没有历史记录可导出".to_string());
    }

    // 创建 ZIP 文件
    let file = std::fs::File::create(&zip_path).map_err(|e| format!("创建 ZIP 文件失败: {}", e))?;

    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    // 读取所有历史记录
    let entries = fs::read_dir(&history_dir).map_err(|e| format!("读取历史目录失败: {}", e))?;

    let mut all_peers = Vec::new();
    let mut config_count = 0;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(history_entry) = serde_json::from_str::<HistoryEntry>(&content) {
                        let base_name = format!(
                            "{}-{}",
                            history_entry.ikuai_comment.replace(" ", "_"),
                            history_entry.ikuai_id
                        );

                        // 添加 WireGuard 配置文件
                        let wg_filename = format!("{}.conf", base_name);
                        zip.start_file(&wg_filename, options)
                            .map_err(|e| format!("添加文件到 ZIP 失败: {}", e))?;
                        zip.write_all(history_entry.wg_config.as_bytes())
                            .map_err(|e| format!("写入文件到 ZIP 失败: {}", e))?;

                        // 添加 Surge 配置文件（如果存在）
                        if let Some(surge_config) = &history_entry.surge_config {
                            let surge_filename = format!("{}_surge.conf", base_name);
                            zip.start_file(&surge_filename, options)
                                .map_err(|e| format!("添加 Surge 文件到 ZIP 失败: {}", e))?;
                            zip.write_all(surge_config.as_bytes())
                                .map_err(|e| format!("写入 Surge 文件到 ZIP 失败: {}", e))?;
                        }

                        // 收集 Peer 配置
                        all_peers.push(history_entry.ikuai_config);
                        config_count += 1;
                    }
                }
            }
        }
    }

    if config_count == 0 {
        return Err("没有找到有效的配置".to_string());
    }

    // 添加 all_peers.txt
    let all_peers_content = all_peers.join("\n");
    zip.start_file("all_peers.txt", options)
        .map_err(|e| format!("添加 all_peers.txt 到 ZIP 失败: {}", e))?;
    zip.write_all(all_peers_content.as_bytes())
        .map_err(|e| format!("写入 all_peers.txt 到 ZIP 失败: {}", e))?;

    // 完成 ZIP 文件
    zip.finish()
        .map_err(|e| format!("完成 ZIP 文件失败: {}", e))?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let win_builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("")
                .fullscreen(false)
                .resizable(false)
                .inner_size(1000.0, 810.0);

            // 仅在 macOS 时设置透明标题栏
            #[cfg(target_os = "macos")]
            let win_builder = win_builder.title_bar_style(TitleBarStyle::Transparent);

            let window = win_builder.build().unwrap();
            // 仅在构建 macOS 时设置背景颜色
            #[cfg(target_os = "macos")]
            {
                use cocoa::appkit::{NSColor, NSWindow};
                use cocoa::base::{id, nil};

                let ns_window = window.ns_window().unwrap() as id;
                unsafe {
                    let bg_color = NSColor::colorWithRed_green_blue_alpha_(
                        nil,
                        102.0 / 255.0,
                        126.0 / 255.0,
                        234.5 / 255.0,
                        1.0,
                    );
                    ns_window.setBackgroundColor_(bg_color);
                }
            }

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            generate_keypair,
            generate_preshared_key,
            private_key_to_public,
            load_env_config,
            get_next_peer_id,
            generate_wg_config,
            generate_ikuai_config,
            generate_surge_config,
            generate_mikrotik_config,
            generate_openwrt_config,
            save_persistent_config,
            load_persistent_config,
            generate_qrcode,
            save_config_to_path,
            save_to_history,
            get_history_list,
            get_history_detail,
            delete_history,
            clear_all_history,
            clear_cached_config,
            export_all_configs_zip,
            save_server_config,
            get_server_list,
            get_server_detail,
            delete_server,
            clear_all_servers,
            get_next_peer_id_for_server,
            update_server_peer_id,
            get_history_list_by_server,
            migrate_old_config_to_server,
            // WebDAV 同步命令
            save_webdav_config,
            load_webdav_config,
            test_webdav_connection,
            sync_to_webdav,
            sync_from_webdav,
            sync_bidirectional_webdav,
            save_last_sync_info,
            load_last_sync_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let result = generate_keypair();
        assert!(result.is_ok());

        let keypair = result.unwrap();

        println!("私钥: {}", keypair.private_key);
        println!("公钥: {}", keypair.public_key);

        // 验证私钥和公钥不相同
        assert_ne!(
            keypair.private_key, keypair.public_key,
            "私钥和公钥不应该相同!"
        );

        // 验证长度 (Base64 编码的 32 字节 = 44 字符)
        assert_eq!(keypair.private_key.len(), 44, "私钥 Base64 长度应该是 44");
        assert_eq!(keypair.public_key.len(), 44, "公钥 Base64 长度应该是 44");
    }

    #[test]
    fn test_private_to_public() {
        let keypair = generate_keypair().unwrap();
        let computed_public = private_key_to_public(keypair.private_key.clone()).unwrap();

        println!("原始公钥: {}", keypair.public_key);
        println!("计算公钥: {}", computed_public);

        assert_eq!(keypair.public_key, computed_public, "公钥应该一致");
    }
}
