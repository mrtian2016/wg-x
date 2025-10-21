// daemon.rs - WireGuard 守护进程核心模块
// 以 root 权限运行,管理 WireGuard 隧道

use crate::daemon_ipc::{
    IpcRequest, IpcResponse, PeerConfigIpc, TunnelConfigIpc, TunnelStatusIpc, DAEMON_SOCKET_PATH,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::{Child, Command};
use std::sync::Arc;
use tokio::sync::Mutex;

// 全局隧道进程管理
lazy_static::lazy_static! {
    static ref DAEMON_TUNNELS: Arc<Mutex<HashMap<String, TunnelProcess>>> = Arc::new(Mutex::new(HashMap::new()));
}

// 隧道进程信息
struct TunnelProcess {
    tunnel_id: String,
    interface_name: String,
    socket_path: String, // 实际的 WireGuard UAPI socket 路径
    process: Child,
    config: TunnelConfigIpc,
}

/// 守护进程主循环
pub async fn run_daemon() -> Result<(), String> {
    println!("启动 wg-x 守护进程...");

    // 检查是否以 root 权限运行
    if !nix::unistd::Uid::effective().is_root() {
        return Err("守护进程必须以 root 权限运行".to_string());
    }

    // 删除旧的 socket 文件(如果存在)
    if std::path::Path::new(DAEMON_SOCKET_PATH).exists() {
        std::fs::remove_file(DAEMON_SOCKET_PATH)
            .map_err(|e| format!("删除旧 socket 文件失败: {}", e))?;
    }

    // 创建 Unix Socket 监听器
    let listener = UnixListener::bind(DAEMON_SOCKET_PATH)
        .map_err(|e| format!("绑定 socket 失败: {}", e))?;

    // 设置 socket 文件权限为 0666,允许普通用户连接
    std::fs::set_permissions(
        DAEMON_SOCKET_PATH,
        std::fs::Permissions::from_mode(0o666),
    )
    .map_err(|e| format!("设置 socket 权限失败: {}", e))?;

    println!("守护进程监听在: {}", DAEMON_SOCKET_PATH);

    // 处理连接
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // 为每个连接创建异步任务
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream).await {
                        eprintln!("处理客户端请求失败: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("接受连接失败: {}", e);
            }
        }
    }

    Ok(())
}

/// 处理客户端请求
async fn handle_client(stream: UnixStream) -> Result<(), String> {
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();

    // 读取一行请求
    reader
        .read_line(&mut request_line)
        .map_err(|e| format!("读取请求失败: {}", e))?;

    // 解析请求
    let request: IpcRequest = serde_json::from_str(&request_line)
        .map_err(|e| format!("解析请求失败: {}", e))?;

    println!("收到请求: method={}, id={}", request.method, request.id);

    // 处理请求
    let response = match request.method.as_str() {
        "start_tunnel" => handle_start_tunnel(request.id.clone(), request.params).await,
        "stop_tunnel" => handle_stop_tunnel(request.id.clone(), request.params).await,
        "get_tunnel_status" => handle_get_tunnel_status(request.id.clone(), request.params).await,
        "list_tunnels" => handle_list_tunnels(request.id.clone()).await,
        "ping" => handle_ping(request.id.clone()).await,
        _ => IpcResponse {
            id: request.id.clone(),
            result: None,
            error: Some(format!("未知的方法: {}", request.method)),
        },
    };

    // 发送响应
    let response_json = serde_json::to_string(&response)
        .map_err(|e| format!("序列化响应失败: {}", e))?;

    let mut writer = stream;
    writer
        .write_all(response_json.as_bytes())
        .map_err(|e| format!("发送响应失败: {}", e))?;

    Ok(())
}

/// 处理启动隧道请求
async fn handle_start_tunnel(request_id: String, params: serde_json::Value) -> IpcResponse {
    let config: TunnelConfigIpc = match serde_json::from_value(params) {
        Ok(c) => c,
        Err(e) => {
            return IpcResponse {
                id: request_id,
                result: None,
                error: Some(format!("解析配置失败: {}", e)),
            };
        }
    };

    // 启动隧道
    match start_tunnel_internal(config).await {
        Ok(_) => IpcResponse {
            id: request_id,
            result: Some(serde_json::json!({"status": "ok"})),
            error: None,
        },
        Err(e) => IpcResponse {
            id: request_id,
            result: None,
            error: Some(e),
        },
    }
}

/// 内部启动隧道逻辑
async fn start_tunnel_internal(config: TunnelConfigIpc) -> Result<(), String> {
    let mut tunnels = DAEMON_TUNNELS.lock().await;

    // 检查是否已存在
    if tunnels.contains_key(&config.tunnel_id) {
        return Err(format!("隧道 {} 已在运行", config.tunnel_id));
    }

    // 检查接口是否已存在
    if interface_exists(&config.interface_name) {
        return Err(format!("接口 {} 已存在", config.interface_name));
    }

    // 使用配置中传入的 wireguard-go 路径,如果无效则尝试查找备用路径
    let wg_go_path = if !config.wireguard_go_path.is_empty() && std::path::Path::new(&config.wireguard_go_path).exists() {
        println!("使用应用传入的 wireguard-go 路径: {}", config.wireguard_go_path);
        config.wireguard_go_path.clone()
    } else {
        println!("应用传入的路径无效或不存在: {}", config.wireguard_go_path);
        println!("尝试在系统路径中查找 wireguard-go...");

        // 尝试在系统路径中查找
        match find_wireguard_go() {
            Ok(path) => {
                println!("在系统路径中找到 wireguard-go: {}", path);
                path
            }
            Err(e) => {
                eprintln!("无法找到 wireguard-go 可执行文件");
                eprintln!("应用传入的路径: {}", config.wireguard_go_path);
                eprintln!("当前工作目录: {:?}", std::env::current_dir());
                return Err(format!(
                    "无法找到 wireguard-go 可执行文件。\n\
                    应用传入的路径不存在: {}\n\
                    系统路径中也未找到: {}\n\
                    \n\
                    解决方案:\n\
                    1. 将 wireguard-go 复制到 /usr/local/bin/\n\
                    2. 或安装 wireguard-go 包: sudo apt install wireguard-tools",
                    config.wireguard_go_path, e
                ));
            }
        }
    };

    println!(
        "启动 WireGuard 隧道: interface={}, wireguard-go={}",
        config.interface_name, wg_go_path
    );

    // 启动 wireguard-go (使用引用避免所有权转移)
    let mut child = Command::new(&wg_go_path)
        .arg("-f")
        .arg(&config.interface_name)
        .spawn()
        .map_err(|e| format!("启动 wireguard-go 失败: {}", e))?;

    // 确定 socket 目录和路径
    let socket_dir = config.socket_dir.as_deref().unwrap_or("/var/run/wireguard");
    let socket_path = format!("{}/{}.sock", socket_dir, config.interface_name);

    println!("等待 WireGuard socket 创建: {}", socket_path);

    // 等待 socket 文件创建，同时检查进程是否存活
    let mut retries = 0;
    while retries < 100 {
        // 检查进程是否还活着
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(format!(
                    "wireguard-go 进程意外退出: {}。请检查日志或手动运行 {} -f {} 查看错误",
                    status, wg_go_path, config.interface_name
                ));
            }
            Ok(None) => {
                // 进程还在运行，检查 socket 是否已创建
                if std::path::Path::new(&socket_path).exists() {
                    println!("Socket 文件已创建: {}", socket_path);
                    break;
                }
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("检查进程状态失败: {}", e));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        retries += 1;
    }

    if !std::path::Path::new(&socket_path).exists() {
        let _ = child.kill();
        return Err(format!(
            "等待超时: WireGuard socket 文件未创建: {}。进程可能启动失败",
            socket_path
        ));
    }

    // 配置接口 (通过 UAPI)
    if let Err(e) = configure_interface(&config, &socket_path).await {
        let _ = child.kill();
        return Err(format!("配置接口失败: {}", e));
    }

    // 使用 netlink 配置 IP 地址和启动接口
    if let Err(e) = configure_interface_ip(&config.interface_name, &config.address).await {
        let _ = child.kill();
        return Err(e);
    }

    // 使用 netlink 配置路由
    for peer in &config.peers {
        for allowed_ip in &peer.allowed_ips {
            if allowed_ip == "0.0.0.0/0" || allowed_ip == "::/0" {
                continue; // 跳过默认路由
            }

            let _ = configure_route(&config.interface_name, allowed_ip).await;
        }
    }

    println!("隧道 {} 启动成功", config.tunnel_id);

    // 保存进程信息
    tunnels.insert(
        config.tunnel_id.clone(),
        TunnelProcess {
            tunnel_id: config.tunnel_id.clone(),
            interface_name: config.interface_name.clone(),
            socket_path: socket_path.clone(),
            process: child,
            config,
        },
    );

    Ok(())
}

/// 配置 WireGuard 接口 (通过 UAPI)
async fn configure_interface(config: &TunnelConfigIpc, socket_path: &str) -> Result<(), String> {
    use std::io::Read;
    use std::os::unix::net::UnixStream;

    // 连接到 UAPI socket
    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| format!("连接 WireGuard socket 失败: {}", e))?;

    // 构建配置命令
    let mut uapi_config = String::from("set=1\n");

    // 私钥
    let private_key_hex = base64_to_hex(&config.private_key)?;
    uapi_config.push_str(&format!("private_key={}\n", private_key_hex));

    // 监听端口
    if let Some(port) = config.listen_port {
        uapi_config.push_str(&format!("listen_port={}\n", port));
    }

    uapi_config.push_str("replace_peers=true\n");

    // Peer 配置
    println!("配置 {} 个 peer(s)", config.peers.len());
    for (i, peer) in config.peers.iter().enumerate() {
        println!("配置 peer #{}: endpoint={:?}", i, peer.endpoint);
        let public_key_hex = base64_to_hex(&peer.public_key)?;
        uapi_config.push_str(&format!("public_key={}\n", public_key_hex));

        if let Some(ref endpoint) = peer.endpoint {
            if !endpoint.is_empty() {
                println!("配置 peer endpoint: {}", endpoint);
                // wireguard-go 的 UAPI 只接受 IP 地址，必须解析域名
                // 使用 spawn_blocking 避免在异步上下文中阻塞
                let endpoint_clone = endpoint.clone();
                let resolved = tokio::task::spawn_blocking(move || {
                    resolve_endpoint_blocking(&endpoint_clone)
                })
                .await
                .map_err(|e| format!("解析任务失败: {}", e))?;

                match resolved {
                    Ok(resolved_endpoint) => {
                        println!("成功解析 endpoint: {} -> {}", endpoint, resolved_endpoint);
                        uapi_config.push_str(&format!("endpoint={}\n", resolved_endpoint));
                    }
                    Err(e) => {
                        // DNS 解析失败，返回错误
                        // WireGuard UAPI 不支持域名，必须解析成功
                        eprintln!("错误: 无法解析 endpoint {}: {}", endpoint, e);
                        return Err(format!(
                            "无法解析 endpoint {}: {}。请检查网络连接和 DNS 配置",
                            endpoint, e
                        ));
                    }
                }
            }
        }

        if let Some(ref psk) = peer.preshared_key {
            if !psk.is_empty() {
                // 验证预共享密钥：不能和公钥相同
                if psk == &peer.public_key {
                    return Err("预共享密钥不能与公钥相同，请重新生成或留空".to_string());
                }
                // 预共享密钥也需要转换为十六进制
                match base64_to_hex(psk) {
                    Ok(psk_hex) => {
                        uapi_config.push_str(&format!("preshared_key={}\n", psk_hex));
                    }
                    Err(e) => {
                        println!("警告: 预共享密钥格式无效，已跳过: {}", e);
                        // 跳过无效的预共享密钥，不影响其他配置
                    }
                }
            }
        }

        if let Some(keepalive) = peer.persistent_keepalive {
            uapi_config.push_str(&format!("persistent_keepalive_interval={}\n", keepalive));
        }

        for allowed_ip in &peer.allowed_ips {
            uapi_config.push_str(&format!("allowed_ip={}\n", allowed_ip));
        }
    }

    uapi_config.push_str("\n");

    println!("发送 UAPI 配置:\n{}", uapi_config);

    // 设置读取超时
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|e| format!("设置超时失败: {}", e))?;

    // 发送配置
    stream
        .write_all(uapi_config.as_bytes())
        .map_err(|e| format!("发送配置失败: {}", e))?;

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
        return Err(format!("配置失败: {}", response));
    }

    Ok(())
}

/// 处理停止隧道请求
async fn handle_stop_tunnel(request_id: String, params: serde_json::Value) -> IpcResponse {
    let tunnel_id: String = match serde_json::from_value(params.get("tunnel_id").cloned().unwrap_or_default()) {
        Ok(id) => id,
        Err(e) => {
            return IpcResponse {
                id: request_id,
                result: None,
                error: Some(format!("解析 tunnel_id 失败: {}", e)),
            };
        }
    };

    match stop_tunnel_internal(&tunnel_id).await {
        Ok(_) => IpcResponse {
            id: request_id,
            result: Some(serde_json::json!({"status": "ok"})),
            error: None,
        },
        Err(e) => IpcResponse {
            id: request_id,
            result: None,
            error: Some(e),
        },
    }
}

/// 内部停止隧道逻辑
async fn stop_tunnel_internal(tunnel_id: &str) -> Result<(), String> {
    let mut tunnels = DAEMON_TUNNELS.lock().await;

    if let Some(mut tunnel) = tunnels.remove(tunnel_id) {
        println!("停止隧道: {}", tunnel_id);

        // 1. 杀死 wireguard-go 进程
        if let Err(e) = tunnel.process.kill() {
            eprintln!("警告: 杀死进程失败: {}", e);
        }

        // 2. 等待进程退出（最多等待 5 秒）
        let mut wait_count = 0;
        while wait_count < 50 {
            match tunnel.process.try_wait() {
                Ok(Some(_)) => {
                    println!("wireguard-go 进程已退出");
                    break;
                }
                Ok(None) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    wait_count += 1;
                }
                Err(e) => {
                    eprintln!("检查进程退出状态失败: {}", e);
                    break;
                }
            }
        }

        // 如果进程仍未退出，强制 kill -9
        if wait_count >= 50 {
            eprintln!("警告: 进程未在 5 秒内退出，尝试强制终止");
            let _ = tunnel.process.wait();
        }

        // 3. 清理网络接口（wireguard-go 正常退出时会自动清理，但以防万一）
        // 检查接口是否还存在
        if interface_exists(&tunnel.interface_name) {
            println!("清理残留的网络接口: {}", tunnel.interface_name);
            if let Err(e) = Command::new("ip")
                .args(["link", "delete", &tunnel.interface_name])
                .output()
            {
                eprintln!("警告: 删除网络接口失败: {}", e);
            }
        }

        // 4. 清理 socket 文件（通常 wireguard-go 会自动清理，但以防万一）
        if std::path::Path::new(&tunnel.socket_path).exists() {
            println!("清理残留的 socket 文件: {}", tunnel.socket_path);
            if let Err(e) = std::fs::remove_file(&tunnel.socket_path) {
                eprintln!("警告: 删除 socket 文件失败: {}", e);
            }
        }

        println!("隧道 {} 已停止并清理完成", tunnel_id);
        Ok(())
    } else {
        Err(format!("隧道 {} 未运行", tunnel_id))
    }
}

/// 处理获取隧道状态请求
async fn handle_get_tunnel_status(request_id: String, params: serde_json::Value) -> IpcResponse {
    println!("收到获取隧道状态请求: params={:?}", params);
    let tunnel_id: String = match serde_json::from_value(params.get("tunnel_id").cloned().unwrap_or_default()) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("解析 tunnel_id 失败: {}", e);
            return IpcResponse {
                id: request_id,
                result: None,
                error: Some(format!("解析 tunnel_id 失败: {}", e)),
            };
        }
    };

    println!("查询隧道状态: tunnel_id={}", tunnel_id);
    match get_tunnel_status_internal(&tunnel_id).await {
        Ok(status) => IpcResponse {
            id: request_id,
            result: Some(serde_json::to_value(&status).unwrap()),
            error: None,
        },
        Err(e) => IpcResponse {
            id: request_id,
            result: None,
            error: Some(e),
        },
    }
}

/// 内部获取隧道状态逻辑
async fn get_tunnel_status_internal(tunnel_id: &str) -> Result<TunnelStatusIpc, String> {
    println!("开始获取隧道 {} 的状态", tunnel_id);
    let socket_path = {
        let tunnels = DAEMON_TUNNELS.lock().await;
        println!("当前运行中的隧道: {:?}", tunnels.keys().collect::<Vec<_>>());

        if let Some(tunnel) = tunnels.get(tunnel_id) {
            println!("找到隧道，socket 路径: {}", tunnel.socket_path);
            tunnel.socket_path.clone()
        } else {
            eprintln!("隧道 {} 未在运行列表中", tunnel_id);
            return Err(format!("隧道 {} 未运行", tunnel_id));
        }
    };

    // 在阻塞线程池中获取统计信息
    println!("准备获取接口统计信息...");
    let socket_path_clone = socket_path.clone();
    let (tx_bytes, rx_bytes, last_handshake) = tokio::task::spawn_blocking(move || {
        println!("在阻塞线程中获取统计: {}", socket_path_clone);
        get_interface_stats(&socket_path_clone)
    })
    .await
    .map_err(|e| format!("获取统计任务失败: {}", e))?
    .unwrap_or((0, 0, None));

    println!("统计信息: tx={}, rx={}", tx_bytes, rx_bytes);

    // 再次获取接口名称（需要重新锁定）
    let interface_name = {
        let tunnels = DAEMON_TUNNELS.lock().await;
        tunnels.get(tunnel_id)
            .map(|t| t.interface_name.clone())
            .ok_or_else(|| "隧道已停止".to_string())?
    };

    Ok(TunnelStatusIpc {
        tunnel_id: tunnel_id.to_string(),
        status: "running".to_string(),
        interface_name,
        tx_bytes,
        rx_bytes,
        last_handshake,
    })
}

/// 获取接口统计信息
fn get_interface_stats(socket_path: &str) -> Result<(u64, u64, Option<i64>), String> {
    use std::io::Read;
    use std::os::unix::net::UnixStream;

    println!("连接到 socket: {}", socket_path);
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|e| format!("连接 socket {} 失败: {}", socket_path, e))?;

    // 设置读取超时
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .map_err(|e| format!("设置超时失败: {}", e))?;

    println!("发送 get 命令");
    stream
        .write_all(b"get=1\n\n")
        .map_err(|e| format!("发送请求失败: {}", e))?;

    // 读取响应 - 读取直到遇到双换行符或超时
    let mut response = String::new();
    let mut buffer = [0u8; 4096];

    println!("开始读取响应");
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                println!("EOF");
                break;
            }
            Ok(n) => {
                response.push_str(&String::from_utf8_lossy(&buffer[..n]));
                // WireGuard UAPI 响应以双换行符结束
                if response.contains("\n\n") {
                    println!("检测到双换行符，停止读取");
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                // 超时或没有更多数据
                if !response.is_empty() {
                    println!("超时但已有数据，停止读取");
                    break;
                }
                return Err("读取超时".to_string());
            }
            Err(e) => return Err(format!("读取失败: {}", e)),
        }
    }

    println!("读取到的响应长度: {}", response.len());

    let mut tx_bytes = 0u64;
    let mut rx_bytes = 0u64;
    let mut last_handshake: Option<i64> = None;

    for line in response.lines() {
        if line.starts_with("rx_bytes=") {
            rx_bytes = line.strip_prefix("rx_bytes=").unwrap_or("0").parse().unwrap_or(0);
        } else if line.starts_with("tx_bytes=") {
            tx_bytes = line.strip_prefix("tx_bytes=").unwrap_or("0").parse().unwrap_or(0);
        } else if line.starts_with("last_handshake_time_sec=") {
            if let Ok(ts) = line.strip_prefix("last_handshake_time_sec=").unwrap_or("0").parse::<i64>() {
                if ts > 0 {
                    last_handshake = Some(ts);
                }
            }
        }
    }

    Ok((tx_bytes, rx_bytes, last_handshake))
}

/// 处理列出隧道请求
async fn handle_list_tunnels(request_id: String) -> IpcResponse {
    let tunnels = DAEMON_TUNNELS.lock().await;
    let tunnel_ids: Vec<String> = tunnels.keys().cloned().collect();

    IpcResponse {
        id: request_id,
        result: Some(serde_json::to_value(&tunnel_ids).unwrap()),
        error: None,
    }
}

/// 处理 ping 请求
async fn handle_ping(request_id: String) -> IpcResponse {
    IpcResponse {
        id: request_id,
        result: Some(serde_json::json!({"status": "pong"})),
        error: None,
    }
}

/// 辅助函数: Base64 转十六进制
fn base64_to_hex(base64_key: &str) -> Result<String, String> {
    let bytes = BASE64
        .decode(base64_key.trim())
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    if bytes.len() != 32 {
        return Err(format!("密钥长度错误: 应为32字节,实际为{}字节", bytes.len()));
    }

    Ok(hex::encode(&bytes))
}

/// 解析 endpoint: 如果包含域名,解析为 IP 地址
/// 解析 endpoint (域名 -> IP)
/// 注意：此函数会执行阻塞的 DNS 查询
fn resolve_endpoint_blocking(endpoint: &str) -> Result<String, String> {
    use std::net::ToSocketAddrs;

    // 尝试解析为 SocketAddr (这是阻塞调用)
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

/// 检查接口是否存在
fn interface_exists(name: &str) -> bool {
    Command::new("ip")
        .args(["link", "show", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 查找 wireguard-go 可执行文件
fn find_wireguard_go() -> Result<String, String> {
    // 尝试常见路径（优先级顺序）
    let paths = vec![
        "/opt/wg-x/wireguard-go",  // 安装守护进程时复制的位置（优先使用）
        "/usr/local/bin/wireguard-go",
        "/usr/bin/wireguard-go",
        "/opt/wireguard-go/wireguard-go",
    ];

    for path in paths {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    // 在 PATH 中查找
    if let Ok(output) = Command::new("which").arg("wireguard-go").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }

    Err("未找到 wireguard-go 可执行文件".to_string())
}

/// 使用 netlink 配置接口 IP 地址和启动接口
async fn configure_interface_ip(interface: &str, address: &str) -> Result<(), String> {
    use futures::stream::TryStreamExt;
    use rtnetlink::{new_connection, IpVersion};
    use std::net::IpAddr;

    // 在当前 async 上下文中执行，不创建新的 runtime
        let (connection, handle, _) = new_connection()
            .map_err(|e| format!("创建 netlink 连接失败: {}", e))?;

        tokio::spawn(connection);

        // 解析地址
        let parts: Vec<&str> = address.split('/').collect();
        if parts.len() != 2 {
            return Err(format!("无效的地址格式: {}", address));
        }

        let ip: IpAddr = parts[0].parse()
            .map_err(|e| format!("解析 IP 地址失败: {}", e))?;
        let prefix_len: u8 = parts[1].parse()
            .map_err(|e| format!("解析前缀长度失败: {}", e))?;

        // 获取接口索引
        let mut links = handle.link().get().match_name(interface.to_string()).execute();
        let link = links
            .try_next()
            .await
            .map_err(|e| format!("获取接口失败: {}", e))?
            .ok_or_else(|| format!("接口不存在: {}", interface))?;

        let index = link.header.index;

        // 添加 IP 地址
        match ip {
            IpAddr::V4(addr) => {
                handle
                    .address()
                    .add(index, addr.into(), prefix_len)
                    .execute()
                    .await
                    .map_err(|e| format!("添加 IPv4 地址失败: {}", e))?;
            }
            IpAddr::V6(addr) => {
                handle
                    .address()
                    .add(index, addr.into(), prefix_len)
                    .execute()
                    .await
                    .map_err(|e| format!("添加 IPv6 地址失败: {}", e))?;
            }
        }

        // 启动接口
        handle
            .link()
            .set(index)
            .up()
            .execute()
            .await
            .map_err(|e| format!("启动接口失败: {}", e))?;

    println!("接口 {} 已配置地址 {} 并启动", interface, address);
    Ok(())
}

/// 使用 netlink 配置路由
async fn configure_route(interface: &str, destination: &str) -> Result<(), String> {
    use futures::stream::TryStreamExt;
    use rtnetlink::new_connection;
    use std::net::IpAddr;

    // 在当前 async 上下文中执行，不创建新的 runtime
        let (connection, handle, _) = new_connection()
            .map_err(|e| format!("创建 netlink 连接失败: {}", e))?;

        tokio::spawn(connection);

        // 解析目标地址
        let parts: Vec<&str> = destination.split('/').collect();
        if parts.len() != 2 {
            return Err(format!("无效的路由格式: {}", destination));
        }

        let ip: IpAddr = parts[0].parse()
            .map_err(|e| format!("解析目标 IP 失败: {}", e))?;
        let prefix_len: u8 = parts[1].parse()
            .map_err(|e| format!("解析前缀长度失败: {}", e))?;

        // 获取接口索引
        let mut links = handle.link().get().match_name(interface.to_string()).execute();
        let link = links
            .try_next()
            .await
            .map_err(|e| format!("获取接口失败: {}", e))?
            .ok_or_else(|| format!("接口不存在: {}", interface))?;

        let index = link.header.index;

        // 添加路由
        match ip {
            IpAddr::V4(addr) => {
                handle
                    .route()
                    .add()
                    .v4()
                    .destination_prefix(addr, prefix_len)
                    .output_interface(index)
                    .execute()
                    .await
                    .map_err(|e| format!("添加 IPv4 路由失败: {}", e))?;
            }
            IpAddr::V6(addr) => {
                handle
                    .route()
                    .add()
                    .v6()
                    .destination_prefix(addr, prefix_len)
                    .output_interface(index)
                    .execute()
                    .await
                    .map_err(|e| format!("添加 IPv6 路由失败: {}", e))?;
            }
        }

    println!("已添加路由: {} -> {}", destination, interface);
    Ok(())
}
