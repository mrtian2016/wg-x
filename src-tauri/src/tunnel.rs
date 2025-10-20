use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use tauri::Manager;
use tokio::sync::Mutex;
use std::sync::Arc;

// 进程包装器，用于统一管理不同类型的子进程
enum ProcessHandle {
    StdProcess(std::process::Child),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    PrivilegedProcess(i32), // 存储 PID,用于 macOS 和 Linux 的权限提升进程
}

impl ProcessHandle {
    fn kill(&mut self) -> Result<(), String> {
        match self {
            ProcessHandle::StdProcess(child) => {
                child.kill().map_err(|e| format!("杀死进程失败: {}", e))
            }
            #[cfg(target_os = "macos")]
            ProcessHandle::PrivilegedProcess(pid) => {
                stop_wireguard_macos(*pid)
            }
            #[cfg(target_os = "linux")]
            ProcessHandle::PrivilegedProcess(pid) => {
                stop_wireguard_linux(*pid)
            }
        }
    }
}

// 全局隧道进程管理
lazy_static::lazy_static! {
    static ref TUNNEL_PROCESSES: Mutex<HashMap<String, ProcessHandle>> = Mutex::new(HashMap::new());
    // 保存隧道的完整配置(包含原始 endpoint 域名),用于定期更新
    static ref TUNNEL_CONFIGS: Mutex<HashMap<String, (String, InterfaceConfig)>> = Mutex::new(HashMap::new());
}

// macOS 启动 WireGuard 隧道（一次性权限请求完成所有操作）
#[cfg(target_os = "macos")]
fn start_wireguard_macos(
    wireguard_path: &str,
    interface_name: &str,
    ip_address: &str,
    routes: &[String],
) -> Result<ProcessHandle, String> {
    // 创建一个完整的 shell 脚本，在一次权限请求中完成所有操作：
    // 1. 启动 wireguard-go
    // 2. 配置 IP 地址
    // 3. 启动接口
    // 4. 修改 socket 权限，让普通用户可以访问

    let escaped_wg_path = wireguard_path.replace('\'', "'\\''");
    let escaped_interface = interface_name.replace('\'', "'\\''");

    // 解析 IP 地址，移除 CIDR 前缀（如果有）
    // macOS ifconfig 不支持 CIDR 格式，只接受纯 IP 地址
    let (ip_only, netmask) = if ip_address.contains('/') {
        let parts: Vec<&str> = ip_address.split('/').collect();
        let ip = parts[0];
        let prefix_len = parts.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(24);

        // 根据前缀长度生成子网掩码
        let mask = if prefix_len == 32 {
            "255.255.255.255".to_string()
        } else if prefix_len == 24 {
            "255.255.255.0".to_string()
        } else if prefix_len == 16 {
            "255.255.0.0".to_string()
        } else if prefix_len == 8 {
            "255.0.0.0".to_string()
        } else {
            // 通用计算
            let mask_value = (!0u32) << (32 - prefix_len);
            format!("{}.{}.{}.{}",
                (mask_value >> 24) & 0xff,
                (mask_value >> 16) & 0xff,
                (mask_value >> 8) & 0xff,
                mask_value & 0xff)
        };
        (ip, mask)
    } else {
        (ip_address, "255.255.255.0".to_string())
    };

    let escaped_ip = ip_only.replace('\'', "'\\''");
    let escaped_netmask = netmask.replace('\'', "'\\''");

    // 获取当前用户 ID，用于设置 socket 文件所有者
    let current_user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    let escaped_user = current_user.replace('\'', "'\\''");

    // 构建完整的 shell 脚本
    // macOS ifconfig 语法: ifconfig <interface> inet <local-ip> <dest-ip> netmask <mask>
    // 对于 WireGuard 点对点接口，本地和目标地址都设为相同的 IP
    // 这是 WireGuard 在 macOS 上的标准做法
    // 修改 socket 文件所有者，让普通用户可以访问
    // 在一次权限请求中完成接口配置和路由配置
    let mut shell_script = format!(
        "'{}' -f '{}' > /tmp/wireguard-go.log 2>&1 & WG_PID=$! && sleep 1 && /usr/sbin/chown '{}' /var/run/wireguard/{}.sock && /sbin/ifconfig '{}' inet '{}' '{}' netmask '{}' && /sbin/ifconfig '{}' up",
        escaped_wg_path,
        escaped_interface,
        escaped_user,
        escaped_interface,
        escaped_interface,
        escaped_ip,
        escaped_ip,
        escaped_netmask,
        escaped_interface
    );

    // 添加路由配置
    // 使用 || true 忽略路由已存在的错误,避免影响 PID 输出
    for route in routes {
        // 跳过全局路由
        if route == "0.0.0.0/0" || route == "::/0" {
            continue;
        }
        let escaped_route = route.replace('\'', "'\\''");
        // 先尝试删除已存在的路由(忽略错误),然后添加新路由(忽略已存在错误)
        shell_script.push_str(&format!(
            " && (/sbin/route delete -inet {} > /dev/null 2>&1 || true) && (/sbin/route add -inet {} -interface '{}' > /dev/null 2>&1 || true)",
            escaped_route, escaped_route, escaped_interface
        ));
    }

    // 最后输出 PID
    shell_script.push_str(" && echo $WG_PID");

    // 使用 osascript 执行脚本，这会触发系统权限对话框
    let applescript = format!(
        "do shell script \"{}\" with administrator privileges",
        shell_script.replace('\"', "\\\"")
    );

    println!("执行 AppleScript 启动隧道");

    // 执行 osascript 来获取权限并启动进程
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&applescript)
        .output()
        .map_err(|e| format!("执行 osascript 失败: {}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        print!("osascript 错误: {}", error_msg);
        return Err(format!("权限请求失败: {}", error_msg));
    }

    // 从 stdout 读取 PID
    let pid_str = String::from_utf8_lossy(&output.stdout);
    let pid: i32 = pid_str.trim().parse()
        .map_err(|e| format!("解析 PID 失败: {} (输出: {})", e, pid_str))?;

    println!("wireguard-go 已启动，PID: {}", pid);

    Ok(ProcessHandle::PrivilegedProcess(pid))
}

// macOS 停止 WireGuard 进程
#[cfg(target_os = "macos")]
fn stop_wireguard_macos(pid: i32) -> Result<(), String> {
    println!("请求管理员权限以停止隧道进程 (PID: {})...", pid);

    // 使用 SIGKILL (-9) 确保进程被强制终止
    let shell_command = format!("/bin/kill -9 {}", pid);

    let applescript = format!(
        "do shell script \"{}\" with administrator privileges",
        shell_command.replace('\"', "\\\"")
    );

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&applescript)
        .output()
        .map_err(|e| format!("执行命令失败: {}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        if error_msg.contains("User canceled") {
            return Err("用户取消了授权".to_string());
        }
        return Err(format!("终止进程失败: {}", error_msg));
    }

    println!("隧道进程已终止");
    Ok(())
}

// Linux: 通过 pkexec 或 sudo 获取权限并一次性完成所有配置
// pkexec 会弹出图形界面授权对话框,类似 macOS 的 osascript
#[cfg(target_os = "linux")]
fn start_wireguard_linux(
    wg_path: &str,
    interface: &str,
    address: &str,
    routes: &[String],
) -> Result<ProcessHandle, String> {
    println!("准备启动 WireGuard 隧道 (Linux)...");

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

    println!("执行命令脚本");

    // 尝试使用 pkexec (图形界面授权)
    let use_pkexec = std::process::Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let output = if use_pkexec {
        println!("使用 pkexec 请求管理员权限...");
        std::process::Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(&shell_script)
            .output()
            .map_err(|e| format!("执行命令失败: {}", e))?
    } else {
        println!("使用 sudo 请求管理员权限(可能需要在终端输入密码)...");
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

    println!("wireguard-go 已启动,PID: {}", pid);

    // 返回包含 PID 的进程句柄
    // 注意: Linux 使用特殊的标记来表示这是通过权限提升启动的进程
    Ok(ProcessHandle::PrivilegedProcess(pid))
}

// 停止 Linux 隧道
#[cfg(target_os = "linux")]
fn stop_wireguard_linux(pid: i32) -> Result<(), String> {
    println!("请求管理员权限以停止隧道进程 (PID: {})...", pid);

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

// 检查接口是否存在
fn interface_exists(name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ifconfig")
            .arg(name)
            .output();

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

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

// 生成接口名称的辅助函数
// 根据 tunnel_id 和操作系统生成固定的接口名称:
// - Linux: tun[数字] (例如 tun0, tun1...)
// - macOS: utun[数字] (例如 utun0, utun1...)
// 使用 tunnel_id 的哈希确保同一个隧道总是使用相同的接口名
// 注意: 此函数只生成名称,不检查接口是否存在,由调用方负责检查和处理
fn generate_interface_name(tunnel_id: &str) -> String {
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

// 将 Base64 编码的密钥转换为十六进制编码
// WireGuard UAPI 需要十六进制编码的密钥
fn base64_to_hex(base64_key: &str) -> Result<String, String> {
    let bytes = BASE64
        .decode(base64_key.trim())
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    if bytes.len() != 32 {
        return Err(format!("密钥长度错误: 应为32字节,实际为{}字节", bytes.len()));
    }

    // 转换为十六进制字符串
    Ok(hex::encode(&bytes))
}

// 解析 endpoint: 如果包含域名,解析为 IP 地址
fn resolve_endpoint(endpoint: &str) -> Result<String, String> {
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
    pub preshared_key: Option<String>,
    pub endpoint: Option<String>,
    pub allowed_ips: String,
    pub persistent_keepalive: Option<u16>,
}

// 隧道配置(用户创建的配置)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    // Interface 配置
    pub private_key: String,
    pub address: String,
    pub listen_port: String,    // 空字符串表示自动
    pub dns: String,
    pub mtu: String,
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
}

// 配置接口（通过 UAPI）
#[tauri::command]
pub async fn configure_interface(
    interface: String,
    config: InterfaceConfig,
) -> Result<String, String> {
    let socket_path = format!("/var/run/wireguard/{}.sock", interface);

    // 在阻塞线程池中执行同步 I/O
    tokio::task::spawn_blocking(move || {
        // 连接到 UAPI socket
        let mut stream = UnixStream::connect(&socket_path)
            .map_err(|e| format!("无法连接到 socket: {}", e))?;

        // 设置超时
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .map_err(|e| format!("设置超时失败: {}", e))?;

        // 构建配置命令
        let mut uapi_config = String::from("set=1\n");

        // 接口配置 - 将 Base64 私钥转换为十六进制
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
            // 将 Base64 公钥转换为十六进制
            let public_key_hex = base64_to_hex(&peer.public_key)?;
            uapi_config.push_str(&format!("public_key={}\n", public_key_hex));

            if let Some(endpoint) = peer.endpoint {
                if !endpoint.is_empty() {
                    // wireguard-go 的 UAPI 需要 IP 地址,不支持域名
                    // 在发送前解析域名为 IP 地址
                    match resolve_endpoint(&endpoint) {
                        Ok(resolved_endpoint) => {
                            println!("解析 endpoint {} -> {}", endpoint, resolved_endpoint);
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
                    // 验证预共享密钥:不能和公钥相同
                    if psk == &peer.public_key {
                        return Err("预共享密钥不能与公钥相同,请重新生成或留空".to_string());
                    }
                    // 预共享密钥也需要转换为十六进制
                    match base64_to_hex(psk) {
                        Ok(psk_hex) => {
                            uapi_config.push_str(&format!("preshared_key={}\n", psk_hex));
                        }
                        Err(e) => {
                            println!("警告: 预共享密钥格式无效,已跳过: {}", e);
                            // 跳过无效的预共享密钥,不影响其他配置
                        }
                    }
                }
            }

            if let Some(keepalive) = peer.persistent_keepalive {
                uapi_config.push_str(&format!("persistent_keepalive_interval={}\n", keepalive));
            }

            // 允许的 IP 地址
            for allowed_ip in peer.allowed_ips {
                uapi_config.push_str(&format!("allowed_ip={}\n", allowed_ip));
            }
        }

        // 结束配置（两个换行符）
        uapi_config.push_str("\n");

        println!("发送 UAPI 配置:\n{}", uapi_config);

        // 发送配置
        stream
            .write_all(uapi_config.as_bytes())
            .map_err(|e| format!("配置写入失败: {}", e))?;

        // 读取响应 - 按块读取直到遇到双换行符
        let mut response = String::new();
        let mut buffer = [0u8; 4096];

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    response.push_str(&String::from_utf8_lossy(&buffer[..n]));
                    // UAPI 响应以 errno=0 或双换行符结束
                    if response.contains("\n\n") || response.contains("errno=") {
                        break;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    if !response.is_empty() {
                        break;
                    }
                    return Err("读取响应超时".to_string());
                }
                Err(e) => return Err(format!("读取响应失败: {}", e)),
            }
        }

        println!("UAPI 响应:\n{}", response);

        if response.contains("errno=") && !response.contains("errno=0") {
            Err(format!("配置失败: {}", response))
        } else {
            Ok("配置应用成功".to_string())
        }
    }).await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// 添加 Peer
#[tauri::command]
pub async fn add_peer(interface: String, peer: PeerConfig) -> Result<String, String> {
    let socket_path = format!("/var/run/wireguard/{}.sock", interface);

    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| format!("无法连接到 socket: {}", e))?;

    let mut uapi_config = String::from("set=1\n");

    // 将 Base64 公钥转换为十六进制
    let public_key_hex = base64_to_hex(&peer.public_key)?;
    uapi_config.push_str(&format!("public_key={}\n", public_key_hex));

    if let Some(endpoint) = peer.endpoint {
        uapi_config.push_str(&format!("endpoint={}\n", endpoint));
    }

    if let Some(psk) = peer.preshared_key {
        // 预共享密钥也需要转换为十六进制
        let psk_hex = base64_to_hex(&psk)?;
        uapi_config.push_str(&format!("preshared_key={}\n", psk_hex));
    }

    if let Some(keepalive) = peer.persistent_keepalive {
        uapi_config.push_str(&format!("persistent_keepalive_interval={}\n", keepalive));
    }

    for allowed_ip in peer.allowed_ips {
        uapi_config.push_str(&format!("allowed_ip={}\n", allowed_ip));
    }

    uapi_config.push_str("\n");

    stream
        .write_all(uapi_config.as_bytes())
        .map_err(|e| format!("配置写入失败: {}", e))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if response.contains("errno=") {
        Err(format!("添加 Peer 失败: {}", response))
    } else {
        Ok("Peer 添加成功".to_string())
    }
}

// 移除 Peer
#[tauri::command]
pub async fn remove_peer(interface: String, public_key: String) -> Result<String, String> {
    let socket_path = format!("/var/run/wireguard/{}.sock", interface);

    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| format!("无法连接到 socket: {}", e))?;

    // 将 Base64 公钥转换为十六进制
    let public_key_hex = base64_to_hex(&public_key)?;
    let uapi_config = format!("set=1\npublic_key={}\nremove=true\n\n", public_key_hex);

    stream
        .write_all(uapi_config.as_bytes())
        .map_err(|e| format!("配置写入失败: {}", e))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if response.contains("errno=") {
        Err(format!("移除 Peer 失败: {}", response))
    } else {
        Ok("Peer 移除成功".to_string())
    }
}

// 获取接口状态
#[tauri::command]
pub async fn get_interface_status(interface: String) -> Result<String, String> {
    let socket_path = format!("/var/run/wireguard/{}.sock", interface);

    // 在 tokio 的阻塞线程池中执行同步 I/O
    tokio::task::spawn_blocking(move || {
        let mut stream = UnixStream::connect(&socket_path)
            .map_err(|e| format!("无法连接到 socket: {}", e))?;

        // 设置读取超时
        stream.set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .map_err(|e| format!("设置超时失败: {}", e))?;

        // 发送 get 命令
        stream
            .write_all(b"get=1\n\n")
            .map_err(|e| format!("写入失败: {}", e))?;

        // 读取状态 - 读取直到遇到双换行符或超时
        let mut response = String::new();
        let mut buffer = [0u8; 4096];

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    response.push_str(&String::from_utf8_lossy(&buffer[..n]));
                    // WireGuard UAPI 响应以双换行符结束
                    if response.contains("\n\n") {
                        break;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    // 超时或没有更多数据
                    if !response.is_empty() {
                        break;
                    }
                    return Err("读取超时".to_string());
                }
                Err(e) => return Err(format!("读取失败: {}", e)),
            }
        }

        Ok(response)
    }).await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// 配置路由表 (macOS)
#[cfg(target_os = "macos")]
fn configure_routes_macos(interface: &str, allowed_ips: &[String]) -> Result<(), String> {
    for ip in allowed_ips {
        // 跳过 0.0.0.0/0 和 ::/0 这样的全局路由,避免影响系统默认路由
        if ip == "0.0.0.0/0" || ip == "::/0" {
            println!("跳过全局路由: {}", ip);
            continue;
        }

        let route_cmd = format!(
            "route add -inet {} -interface {}",
            ip, interface
        );

        let shell_script = format!(
            "{}",
            route_cmd
        );

        let applescript = format!(
            "do shell script \"{}\" with administrator privileges",
            shell_script.replace('\"', "\\\"")
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&applescript)
            .output()
            .map_err(|e| format!("执行路由配置失败: {}", e))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            println!("添加路由失败 ({}): {}", ip, error_msg);
            // 不中断流程,继续添加其他路由
        } else {
            println!("已添加路由: {} -> {}", ip, interface);
        }
    }

    Ok(())
}

// 配置路由表 (Linux)
#[cfg(target_os = "linux")]
fn configure_routes_linux(interface: &str, allowed_ips: &[String]) -> Result<(), String> {
    for ip in allowed_ips {
        // 跳过全局路由
        if ip == "0.0.0.0/0" || ip == "::/0" {
            println!("跳过全局路由: {}", ip);
            continue;
        }

        let output = std::process::Command::new("sudo")
            .args(["ip", "route", "add", ip, "dev", interface])
            .output()
            .map_err(|e| format!("执行路由配置失败: {}", e))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            println!("添加路由失败 ({}): {}", ip, error_msg);
        } else {
            println!("已添加路由: {} -> {}", ip, interface);
        }
    }

    Ok(())
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
        return Err(format!("接口 {} 已存在,可能有残留进程。请先手动停止或删除该接口", potential_interface));
    }

    // 从隧道配置目录加载配置
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config_file = app_data_dir.join("tunnels").join(format!("{}.json", tunnel_id));

    if !config_file.exists() {
        return Err("隧道配置不存在".to_string());
    }

    let content = std::fs::read_to_string(&config_file)
        .map_err(|e| format!("读取配置失败: {}", e))?;

    let tunnel_config: TunnelConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败: {}", e))?;

    // 生成接口名称
    let interface_name = generate_interface_name(&tunnel_id);

    println!("interface name: {}", interface_name);

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
            let allowed_ips: Vec<String> = tunnel_peer.allowed_ips
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
            tunnel_config.allowed_ips.split(',').map(|s| s.trim().to_string()).collect()
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

    // 获取 wireguard-go sidecar 的路径
    let sidecar_path = app
        .path()
        .resolve("wireguard-go", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("获取 sidecar 路径失败: {}", e))?;

    let sidecar_path_str = sidecar_path
        .to_str()
        .ok_or_else(|| "无法转换 sidecar 路径".to_string())?;

    // macOS: 一次性权限请求，完成所有配置（包括路由）
    #[cfg(target_os = "macos")]
    {
        let process_handle = start_wireguard_macos(
            sidecar_path_str,
            &interface_name,
            &tunnel_config.address,
            &all_routes,
        ).map_err(|e| format!("启动隧道失败: {}", e))?;

        // 保存进程句柄
        {
            let mut processes = TUNNEL_PROCESSES.lock().await;
            processes.insert(tunnel_id.clone(), process_handle);
        }

        // 等待 socket 文件创建（最多等待 5 秒）
        let socket_path = format!("/var/run/wireguard/{}.sock", interface_name);
        let mut retries = 0;
        let max_retries = 50;

        while retries < max_retries {
            if std::path::Path::new(&socket_path).exists() {
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            retries += 1;
        }

        if !std::path::Path::new(&socket_path).exists() {
            let _ = stop_tunnel(tunnel_id.clone()).await;
            return Err(format!(
                "wireguard-go 启动超时。socket 文件未创建: {}",
                socket_path
            ));
        }
    }

    // Linux: 一次性权限请求，完成所有配置（包括路由）
    #[cfg(target_os = "linux")]
    {
        let process_handle = start_wireguard_linux(
            sidecar_path_str,
            &interface_name,
            &tunnel_config.address,
            &all_routes,
        ).map_err(|e| format!("启动隧道失败: {}", e))?;

        // 保存进程句柄
        {
            let mut processes = TUNNEL_PROCESSES.lock().await;
            processes.insert(tunnel_id.clone(), process_handle);
        }

        // 等待 socket 文件创建（最多等待 8 秒,Linux 需要更长时间）
        let socket_path = format!("/var/run/wireguard/{}.sock", interface_name);
        let mut retries = 0;
        let max_retries = 80;

        while retries < max_retries {
            if std::path::Path::new(&socket_path).exists() {
                // socket 文件已创建,额外等待权限设置完成
                println!("socket 文件已创建,等待权限设置...");
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            retries += 1;
        }

        if !std::path::Path::new(&socket_path).exists() {
            let _ = stop_tunnel(tunnel_id.clone()).await;
            return Err(format!(
                "wireguard-go 启动超时。socket 文件未创建: {}",
                socket_path
            ));
        }
    }

    match configure_interface(interface_name.clone(), interface_config.clone()).await {
        Ok(_) => {
            println!("接口配置成功");

            // 保存隧道配置(用于定期更新 endpoint)
            {
                let mut configs = TUNNEL_CONFIGS.lock().await;
                configs.insert(tunnel_id.clone(), (interface_name.clone(), interface_config.clone()));
            }

            // 启动 endpoint 定期刷新任务(处理动态域名)
            start_endpoint_refresh_task(tunnel_id.clone(), interface_name.clone());
            println!("已启动 endpoint 定期刷新任务");

            // macOS 和 Linux: 路由和接口配置已在启动函数中完成,无需额外操作

            println!("隧道启动完成: {}", interface_name);
            Ok(())
        }
        Err(e) => {
            // 配置失败，停止进程
            let _ = stop_tunnel(tunnel_id).await;
            Err(format!("配置接口失败: {}", e))
        }
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
            println!("已清理隧道配置,endpoint 刷新任务将自动停止");
        }

        child
            .kill()
            .map_err(|e| format!("停止隧道失败: {}", e))?;
        Ok(())
    } else {
        // 即使进程不在列表中,也检查接口是否存在并尝试清理
        let interface_name = generate_interface_name(&tunnel_id);
        if interface_exists(&interface_name) {
            println!("检测到残留接口 {},尝试清理...", interface_name);

            // 接口存在但进程不在列表中,可能是残留的接口
            // 尝试通过杀死 wireguard-go 进程来清理
            #[cfg(target_os = "macos")]
            {
                // 使用 osascript 请求管理员权限来杀死进程
                let shell_command = format!("/usr/bin/pkill -9 -f 'wireguard-go.*{}'", interface_name);

                let applescript = format!(
                    "do shell script \"{}\" with administrator privileges",
                    shell_command.replace('\"', "\\\"")
                );

                println!("请求管理员权限以停止隧道...");

                let output = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(&applescript)
                    .output();

                match output {
                    Ok(result) => {
                        if result.status.success() {
                            println!("已发送终止信号给 wireguard-go 进程");
                        } else {
                            let error_msg = String::from_utf8_lossy(&result.stderr);
                            println!("终止进程失败: {}", error_msg);
                            if error_msg.contains("User canceled") {
                                return Err("用户取消了授权".to_string());
                            }
                        }
                    }
                    Err(e) => {
                        println!("执行命令失败: {}", e);
                    }
                }
            }

            #[cfg(target_os = "linux")]
            {
                // 使用 pkexec 或 sudo 请求管理员权限来杀死进程
                let shell_command = format!("/usr/bin/pkill -9 -f 'wireguard-go.*{}'", interface_name);

                // 尝试使用 pkexec
                let use_pkexec = std::process::Command::new("which")
                    .arg("pkexec")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                println!("请求管理员权限以停止隧道...");

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
                            println!("已发送终止信号给 wireguard-go 进程");
                        } else {
                            let error_msg = String::from_utf8_lossy(&result.stderr);
                            println!("终止进程失败: {}", error_msg);
                        }
                    }
                    Err(e) => {
                        println!("执行命令失败: {}", e);
                    }
                }
            }

            // 等待进程终止
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // 检查接口是否已被清理
            if !interface_exists(&interface_name) {
                println!("接口已成功清理");
                return Ok(());
            } else {
                println!("接口仍然存在,可能需要手动清理");
                return Err(format!("已尝试清理残留接口 {},但仍然存在。请检查系统进程或重启应用", interface_name));
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
pub async fn get_tunnel_details(tunnel_id: String, app: tauri::AppHandle) -> Result<TunnelStatus, String> {
    // 获取应用数据目录
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    // 从隧道配置目录加载配置
    let config_file = app_data_dir.join("tunnels").join(format!("{}.json", tunnel_id));

    if !config_file.exists() {
        return Err("隧道配置不存在".to_string());
    }

    let content = std::fs::read_to_string(&config_file)
        .map_err(|e| format!("读取配置失败: {}", e))?;

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
        let status_str = get_interface_status(interface_name)
            .await
            .unwrap_or_default();
        parse_interface_status(&status_str)
    } else {
        (0, 0, None)
    };

    // 从 peers 数组或旧格式字段中提取 endpoint 和 allowed_ips
    let (endpoint, allowed_ips) = if !tunnel_config.peers.is_empty() {
        let first_peer = &tunnel_config.peers[0];
        (
            first_peer.endpoint.clone(),
            Some(first_peer.allowed_ips.clone())
        )
    } else {
        (
            if tunnel_config.endpoint.is_empty() { None } else { Some(tunnel_config.endpoint.clone()) },
            if tunnel_config.allowed_ips.is_empty() { None } else { Some(tunnel_config.allowed_ips.clone()) }
        )
    };

    // 计算公钥 (如果有私钥的话)
    let public_key = if !tunnel_config.private_key.is_empty() {
        crate::private_key_to_public(tunnel_config.private_key.clone()).ok()
    } else {
        None
    };

    Ok(TunnelStatus {
        id: tunnel_id,
        name: tunnel_config.name.clone(),
        status: if is_running { "running".to_string() } else { "stopped".to_string() },
        address: Some(tunnel_config.address.clone()),
        endpoint,
        listen_port: tunnel_config.listen_port.parse().ok(),
        tx_bytes,
        rx_bytes,
        last_handshake,
        public_key,
        allowed_ips,
    })
}

// 解析 WireGuard 配置文件
fn parse_wg_config(config: &str) -> Result<InterfaceConfig, String> {
    let mut private_key = String::new();
    let mut listen_port = None;
    let mut peers = Vec::new();
    let mut current_peer: Option<PeerConfig> = None;

    for line in config.lines() {
        let line = line.trim();

        if line.starts_with("PrivateKey") {
            if let Some(value) = line.split('=').nth(1) {
                private_key = value.trim().to_string();
            }
        } else if line.starts_with("ListenPort") {
            if let Some(value) = line.split('=').nth(1) {
                listen_port = value.trim().parse().ok();
            }
        } else if line.starts_with("[Peer]") {
            if let Some(peer) = current_peer.take() {
                peers.push(peer);
            }
            current_peer = Some(PeerConfig {
                public_key: String::new(),
                endpoint: None,
                allowed_ips: Vec::new(),
                persistent_keepalive: None,
                preshared_key: None,
            });
        } else if let Some(ref mut peer) = current_peer {
            if line.starts_with("PublicKey") {
                if let Some(value) = line.split('=').nth(1) {
                    peer.public_key = value.trim().to_string();
                }
            } else if line.starts_with("Endpoint") {
                if let Some(value) = line.split('=').nth(1) {
                    peer.endpoint = Some(value.trim().to_string());
                }
            } else if line.starts_with("AllowedIPs") {
                if let Some(value) = line.split('=').nth(1) {
                    peer.allowed_ips = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
            } else if line.starts_with("PersistentKeepalive") {
                if let Some(value) = line.split('=').nth(1) {
                    peer.persistent_keepalive = value.trim().parse().ok();
                }
            } else if line.starts_with("PresharedKey") {
                if let Some(value) = line.split('=').nth(1) {
                    peer.preshared_key = Some(value.trim().to_string());
                }
            }
        }
    }

    if let Some(peer) = current_peer {
        peers.push(peer);
    }

    if private_key.is_empty() {
        return Err("未找到私钥".to_string());
    }

    Ok(InterfaceConfig {
        private_key,
        listen_port,
        fwmark: None,
        replace_peers: true,
        peers,
    })
}

// 解析接口状态
fn parse_interface_status(status: &str) -> (u64, u64, Option<i64>) {
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

// 从配置中提取 Endpoint
fn extract_endpoint(config: &str) -> String {
    for line in config.lines() {
        if line.starts_with("Endpoint") {
            if let Some(value) = line.split('=').nth(1) {
                return value.trim().to_string();
            }
        }
    }
    "Unknown".to_string()
}

// 从配置中提取 AllowedIPs
fn extract_allowed_ips(config: &str) -> String {
    for line in config.lines() {
        if line.starts_with("AllowedIPs") {
            if let Some(value) = line.split('=').nth(1) {
                return value.trim().to_string();
            }
        }
    }
    "Unknown".to_string()
}

// ========== 新的隧道配置管理命令 ==========

// 保存隧道配置
#[tauri::command]
pub async fn save_tunnel_config(app: tauri::AppHandle, config: TunnelConfig) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let tunnels_dir = app_data_dir.join("tunnels");
    std::fs::create_dir_all(&tunnels_dir).map_err(|e| format!("创建隧道目录失败: {}", e))?;

    let file_path = tunnels_dir.join(format!("{}.json", config.id));
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("序列化隧道配置失败: {}", e))?;

    std::fs::write(&file_path, json).map_err(|e| format!("保存隧道配置失败: {}", e))?;

    Ok(())
}

// 获取隧道完整配置(用于编辑)
#[tauri::command]
pub async fn get_tunnel_config(app: tauri::AppHandle, tunnel_id: String) -> Result<TunnelConfig, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let config_file = app_data_dir.join("tunnels").join(format!("{}.json", tunnel_id));

    if !config_file.exists() {
        return Err("隧道配置不存在".to_string());
    }

    let content = std::fs::read_to_string(&config_file)
        .map_err(|e| format!("读取配置失败: {}", e))?;

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

    let file_path = app_data_dir.join("tunnels").join(format!("{}.json", tunnel_id));

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

    println!("检查隧道目录: {:?}", tunnels_dir);

    if !tunnels_dir.exists() {
        println!("隧道目录不存在");
        return Ok(Vec::new());
    }

    let mut tunnels = Vec::new();

    // 获取运行中的隧道 ID 列表
    let running_tunnels: Vec<String> = {
        let processes = TUNNEL_PROCESSES.lock().await;
        processes.keys().cloned().collect()
    };

    // 读取所有隧道配置
    let entries = std::fs::read_dir(&tunnels_dir)
        .map_err(|e| format!("读取隧道目录失败: {}", e))?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            // println!("检查文件: {:?}", path);
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // println!("读取配置成功,长度: {}", content.len());
                    match serde_json::from_str::<TunnelConfig>(&content) {
                        Ok(tunnel_config) => {
                            println!("解析配置成功: id={}, name={}", tunnel_config.id, tunnel_config.name);
                        let is_in_process_list = running_tunnels.contains(&tunnel_config.id);

                        // 生成接口名称
                        let interface_name = generate_interface_name(&tunnel_config.id);
                        let interface_exists = interface_exists(&interface_name);

                        // 判断实际运行状态:
                        // 1. 在进程列表 + 接口存在 = running
                        // 2. 不在进程列表 + 接口存在 = running (残留进程,但仍在运行)
                        // 3. 不在进程列表 + 接口不存在 = stopped
                        let is_running = is_in_process_list || interface_exists;

                        let (tx_bytes, rx_bytes, last_handshake) = if is_running {
                            println!("获取接口 {} 状态...", interface_name);
                            // 使用超时避免阻塞
                            let timeout = tokio::time::Duration::from_secs(2);
                            let status_result = tokio::time::timeout(
                                timeout,
                                get_interface_status(interface_name.clone())
                            ).await;

                            match status_result {
                                Ok(Ok(status_str)) => {
                                    println!("获取状态成功");
                                    parse_interface_status(&status_str)
                                }
                                Ok(Err(e)) => {
                                    println!("获取状态失败: {}", e);
                                    (0, 0, None)
                                }
                                Err(_) => {
                                    println!("获取状态超时");
                                    (0, 0, None)
                                }
                            }
                        } else {
                            (0, 0, None)
                        };

                        // 从 peers 数组或旧格式字段中提取 endpoint 和 allowed_ips
                        let (endpoint, allowed_ips) = if !tunnel_config.peers.is_empty() {
                            // 使用新格式: peers 数组 (取第一个 peer 的信息用于显示)
                            let first_peer = &tunnel_config.peers[0];
                            (
                                first_peer.endpoint.clone(),
                                Some(first_peer.allowed_ips.clone())
                            )
                        } else {
                            // 向后兼容: 使用旧格式字段
                            (
                                if tunnel_config.endpoint.is_empty() { None } else { Some(tunnel_config.endpoint.clone()) },
                                if tunnel_config.allowed_ips.is_empty() { None } else { Some(tunnel_config.allowed_ips.clone()) }
                            )
                        };

                        let tunnel_status = TunnelStatus {
                            id: tunnel_config.id.clone(),
                            name: tunnel_config.name.clone(),
                            status: if is_running { "running".to_string() } else { "stopped".to_string() },
                            address: Some(tunnel_config.address.clone()),
                            endpoint,
                            listen_port: tunnel_config.listen_port.parse().ok(),
                            tx_bytes,
                            rx_bytes,
                            last_handshake,
                            public_key: None, // 不暴露公钥
                            allowed_ips,
                        };

                        // println!("添加隧道: {:?}", tunnel_status.name);
                        tunnels.push(tunnel_status);
                        }
                        Err(e) => {
                            println!("解析配置失败: {}", e);
                        }
                    }
                } else {
                    println!("读取文件失败");
                }
            }
        }
    }

    // println!("共找到 {} 个隧道", tunnels.len());

    // 按创建时间降序排序
    tunnels.sort_by(|a, b| b.id.cmp(&a.id));

    Ok(tunnels)
}

// 定期更新 endpoint 的后台任务
// 用于处理动态域名(DDNS)的情况
pub fn start_endpoint_refresh_task(tunnel_id: String, interface: String) {
    tokio::spawn(async move {
        // 每 2 分钟检查一次 endpoint
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(120));

        // 保存每个 peer 上次解析的 endpoint,避免重复更新
        let mut last_resolved_endpoints: HashMap<String, String> = HashMap::new();

        loop {
            interval.tick().await;

            // 检查隧道是否还在运行
            let config_opt = {
                let processes = TUNNEL_PROCESSES.lock().await;
                if !processes.contains_key(&tunnel_id) {
                    println!("隧道 {} 已停止,结束 endpoint 刷新任务", tunnel_id);
                    break;
                }

                // 获取保存的配置
                let configs = TUNNEL_CONFIGS.lock().await;
                configs.get(&tunnel_id).cloned()
            };

            if let Some((iface, config)) = config_opt {
                if iface != interface {
                    println!("接口名称不匹配,跳过更新");
                    continue;
                }

                // 遍历所有 peer,检查并更新 endpoint
                for peer in &config.peers {
                    if let Some(ref original_endpoint) = peer.endpoint {
                        if original_endpoint.is_empty() {
                            continue;
                        }

                        // 重新解析域名
                        match resolve_endpoint(original_endpoint) {
                            Ok(resolved_endpoint) => {
                                // 检查 IP 是否变化
                                let last_endpoint = last_resolved_endpoints.get(&peer.public_key);

                                if let Some(last) = last_endpoint {
                                    if last == &resolved_endpoint {
                                        // IP 没有变化,跳过更新
                                        continue;
                                    }
                                }

                                println!(
                                    "隧道 {}: endpoint {} 解析结果变化: {} -> {}",
                                    tunnel_id,
                                    original_endpoint,
                                    last_endpoint.unwrap_or(&"(首次)".to_string()),
                                    resolved_endpoint
                                );

                                // 更新 endpoint (只更新这个 peer 的 endpoint)
                                let public_key_hex = match base64_to_hex(&peer.public_key) {
                                    Ok(hex) => hex,
                                    Err(e) => {
                                        println!("解析公钥失败: {}", e);
                                        continue;
                                    }
                                };

                                // 构建 UAPI 更新命令
                                let update_config = format!(
                                    "set=1\npublic_key={}\nendpoint={}\n\n",
                                    public_key_hex, resolved_endpoint
                                );

                                // 发送更新到 socket
                                let socket_path = format!("/var/run/wireguard/{}.sock", interface);
                                let result = tokio::task::spawn_blocking(move || {
                                    let mut stream = match UnixStream::connect(&socket_path) {
                                        Ok(s) => s,
                                        Err(e) => {
                                            println!("连接 socket 失败: {}", e);
                                            return Err(format!("连接失败: {}", e));
                                        }
                                    };

                                    stream
                                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                                        .ok();

                                    stream.write_all(update_config.as_bytes()).ok();

                                    let mut response = String::new();
                                    let mut buffer = [0u8; 1024];
                                    match stream.read(&mut buffer) {
                                        Ok(n) => {
                                            response.push_str(&String::from_utf8_lossy(&buffer[..n]));
                                        }
                                        Err(_) => {}
                                    }

                                    Ok(response)
                                })
                                .await;

                                match result {
                                    Ok(Ok(response)) => {
                                        if response.contains("errno=0") || response.is_empty() {
                                            println!("成功更新 endpoint: {}", resolved_endpoint);
                                            // 保存新的 endpoint,下次对比时使用
                                            last_resolved_endpoints.insert(peer.public_key.clone(), resolved_endpoint);
                                        } else {
                                            println!("更新 endpoint 返回: {}", response);
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        println!("更新 endpoint 失败: {}", e);
                                    }
                                    Err(e) => {
                                        println!("任务执行失败: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("解析 endpoint {} 失败: {}", original_endpoint, e);
                            }
                        }
                    }
                }
            }
        }
    });
}
