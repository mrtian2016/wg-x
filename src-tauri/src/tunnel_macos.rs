use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use crate::tunnel::{
    base64_to_hex, interface_exists, parse_interface_status,
    resolve_endpoint, InterfaceConfig, ProcessHandle, TunnelConfig,
    TUNNEL_CONFIGS, TUNNEL_PROCESSES,
};

// macOS 启动 WireGuard 隧道（一次性权限请求完成所有操作）
pub fn start_wireguard_macos(
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
        let prefix_len = parts
            .get(1)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(24);

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
            format!(
                "{}.{}.{}.{}",
                (mask_value >> 24) & 0xff,
                (mask_value >> 16) & 0xff,
                (mask_value >> 8) & 0xff,
                mask_value & 0xff
            )
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

    log::info!("执行 AppleScript 启动隧道");

    // 执行 osascript 来获取权限并启动进程
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&applescript)
        .output()
        .map_err(|e| format!("执行 osascript 失败: {}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        log::error!("osascript 错误: {}", error_msg);
        return Err(format!("权限请求失败: {}", error_msg));
    }

    // 从 stdout 读取 PID
    let pid_str = String::from_utf8_lossy(&output.stdout);
    let pid: i32 = pid_str
        .trim()
        .parse()
        .map_err(|e| format!("解析 PID 失败: {} (输出: {})", e, pid_str))?;

    log::info!("wireguard-go 已启动，PID: {}", pid);

    Ok(ProcessHandle::PrivilegedProcess(pid))
}

// macOS 停止 WireGuard 进程
pub fn stop_wireguard_macos(pid: i32) -> Result<(), String> {
    log::info!("请求管理员权限以停止隧道进程 (PID: {})...", pid);

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

    log::info!("隧道进程已终止");
    Ok(())
}

// macOS 实现：配置接口（通过 UAPI）
pub async fn configure_interface(
    interface: String,
    config: InterfaceConfig,
) -> Result<String, String> {
    let socket_path = format!("/var/run/wireguard/{}.sock", interface);

    // 在阻塞线程池中执行同步 I/O
    tokio::task::spawn_blocking(move || {
        // 连接到 UAPI socket
        let mut stream =
            UnixStream::connect(&socket_path).map_err(|e| format!("无法连接到 socket: {}", e))?;

        // 设置超时
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
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
                            log::warn!("警告: 预共享密钥格式无效,已跳过: {}", e);
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

        log::info!("发送 UAPI 配置:\n{}", uapi_config);

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
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    if !response.is_empty() {
                        break;
                    }
                    return Err("读取响应超时".to_string());
                }
                Err(e) => return Err(format!("读取响应失败: {}", e)),
            }
        }

        log::info!("UAPI 响应:\n{}", response);

        if response.contains("errno=") && !response.contains("errno=0") {
            Err(format!("配置失败: {}", response))
        } else {
            Ok("配置应用成功".to_string())
        }
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// macOS: 获取接口状态
pub async fn get_interface_status(interface: String) -> Result<String, String> {
    let socket_path = format!("/var/run/wireguard/{}.sock", interface);

    // 在 tokio 的阻塞线程池中执行同步 I/O
    tokio::task::spawn_blocking(move || {
        let mut stream =
            UnixStream::connect(&socket_path).map_err(|e| format!("无法连接到 socket: {}", e))?;

        // 设置读取超时
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
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
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
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
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// macOS: 获取隧道状态的实现
pub async fn get_tunnel_status_impl(
    _tunnel_id: &str,
    interface_name: &str,
) -> (u64, u64, Option<i64>) {
    let status_str = get_interface_status(interface_name.to_string())
        .await
        .unwrap_or_default();
    parse_interface_status(&status_str)
}

// macOS: 启动隧道的平台特定部分
pub async fn start_tunnel_platform(
    tunnel_id: String,
    tunnel_config: &TunnelConfig,
    interface_config: &InterfaceConfig,
    interface_name: String,
    all_routes: Vec<String>,
    sidecar_path_str: &str,
) -> Result<(), String> {
    let process_handle = start_wireguard_macos(
        sidecar_path_str,
        &interface_name,
        &tunnel_config.address,
        &all_routes,
    )
    .map_err(|e| format!("启动隧道失败: {}", e))?;

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
        let _ = crate::tunnel::stop_tunnel(tunnel_id.clone()).await;
        return Err(format!(
            "wireguard-go 启动超时。socket 文件未创建: {}",
            socket_path
        ));
    }

    // macOS: 需要 GUI 应用自己配置接口（因为使用的是特权提升方式，不是守护进程）
    match configure_interface(interface_name.clone(), interface_config.clone()).await {
        Ok(_) => {
            log::info!("接口配置成功");

            // 保存隧道配置(用于定期更新 endpoint)
            {
                let mut configs = TUNNEL_CONFIGS.lock().await;
                configs.insert(
                    tunnel_id.clone(),
                    (interface_name.clone(), interface_config.clone()),
                );
            }

            // 启动 endpoint 定期刷新任务(处理动态域名)
            start_endpoint_refresh_task(tunnel_id.clone(), interface_name.clone());
            log::info!("已启动 endpoint 定期刷新任务");

            log::info!("隧道启动完成: {}", interface_name);
            Ok(())
        }
        Err(e) => {
            // 配置失败，停止进程
            let _ = crate::tunnel::stop_tunnel(tunnel_id).await;
            Err(format!("配置接口失败: {}", e))
        }
    }
}

// macOS: 停止隧道的清理逻辑
pub async fn cleanup_stale_tunnel(interface_name: &str) -> Result<(), String> {
    // 使用 osascript 请求管理员权限来杀死进程
    let shell_command = format!("/usr/bin/pkill -9 -f 'wireguard-go.*{}'", interface_name);

    let applescript = format!(
        "do shell script \"{}\" with administrator privileges",
        shell_command.replace('\"', "\\\"")
    );

    log::info!("请求管理员权限以停止隧道...");

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&applescript)
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                log::info!("已发送终止信号给 wireguard-go 进程");
            } else {
                let error_msg = String::from_utf8_lossy(&result.stderr);
                log::error!("终止进程失败: {}", error_msg);
                if error_msg.contains("User canceled") {
                    return Err("用户取消了授权".to_string());
                }
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
                    log::info!("隧道 {} 已停止,结束 endpoint 刷新任务", tunnel_id);
                    break;
                }

                // 获取保存的配置
                let configs = TUNNEL_CONFIGS.lock().await;
                configs.get(&tunnel_id).cloned()
            };

            if let Some((iface, config)) = config_opt {
                if iface != interface {
                    log::debug!("接口名称不匹配,跳过更新");
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

                                log::info!(
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
                                        log::error!("解析公钥失败: {}", e);
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
                                            log::error!("连接 socket 失败: {}", e);
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
                                            response
                                                .push_str(&String::from_utf8_lossy(&buffer[..n]));
                                        }
                                        Err(_) => {}
                                    }

                                    Ok(response)
                                })
                                .await;

                                match result {
                                    Ok(Ok(response)) => {
                                        if response.contains("errno=0") || response.is_empty() {
                                            log::info!("成功更新 endpoint: {}", resolved_endpoint);
                                            // 保存新的 endpoint,下次对比时使用
                                            last_resolved_endpoints
                                                .insert(peer.public_key.clone(), resolved_endpoint);
                                        } else {
                                            log::warn!("更新 endpoint 返回: {}", response);
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        log::warn!("更新 endpoint 失败: {}", e);
                                    }
                                    Err(e) => {
                                        log::warn!("任务执行失败: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("解析 endpoint {} 失败: {}", original_endpoint, e);
                            }
                        }
                    }
                }
            }
        }
    });
}
