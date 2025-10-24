use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::fs;
use tauri::command;

#[command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[command]
pub fn generate_qrcode(content: String) -> Result<String, String> {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let code = QrCode::new(content.as_bytes()).map_err(|e| format!("生成二维码失败: {}", e))?;

    let svg = code.render::<svg::Color>().min_dimensions(200, 200).build();

    let data_url = format!(
        "data:image/svg+xml;base64,{}",
        BASE64.encode(svg.as_bytes())
    );

    Ok(data_url)
}

#[command]
pub fn save_config_to_path(content: String, file_path: String) -> Result<(), String> {
    fs::write(&file_path, content).map_err(|e| format!("保存文件失败: {}", e))?;
    Ok(())
}

#[command]
pub fn read_file_content(file_path: String) -> Result<String, String> {
    fs::read_to_string(&file_path)
        .map_err(|e| format!("读取文件失败: {}", e))
}

#[command]
pub fn read_file_as_base64(file_path: String) -> Result<String, String> {
    let data = fs::read(&file_path)
        .map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(BASE64.encode(&data))
}

#[command]
pub fn get_local_ip() -> Result<String, String> {
    // 获取设备的本地局域网 IP 地址
    // 通过创建一个 UDP 连接到公共 DNS 服务器来获取本地 IP
    // 这个方法不会实际发送数据，只是用来确定出站接口

    use std::net::UdpSocket;

    // 创建一个 UDP socket 连接到公共 DNS 服务器（8.8.8.8:80）
    // 这不会实际建立连接，但会让操作系统选择合适的出站接口
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("创建 socket 失败: {}", e))?;

    socket.connect("8.8.8.8:80")
        .map_err(|e| format!("获取本地 IP 失败: {}", e))?;

    let local_addr = socket.local_addr()
        .map_err(|e| format!("获取本地地址失败: {}", e))?;

    Ok(local_addr.ip().to_string())
}

#[command]
pub fn get_all_local_ips() -> Result<Vec<String>, String> {
    // 获取设备的所有本地局域网 IP 地址（排除虚拟设备）

    let mut ips = Vec::new();

    // 虚拟网络设备的关键字列表
    let virtual_device_keywords = vec![
        "wireguard", "wg", "tun", "tap", "docker", "veth", "br-",
        "vlan", "bond", "tunnel", "vpn", "ppp", "l2tp", "ipsec",
        "loopback", "lo", "utun", "utap",
    ];

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;

        // Windows: 使用 ipconfig 命令并解析接口名称
        if let Ok(output) = Command::new("ipconfig").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut current_adapter = String::new();

            for line in stdout.lines() {
                // 获取当前适配器名称
                if line.starts_with("Ethernet adapter") || line.starts_with("Wireless LAN adapter") {
                    current_adapter = line.to_lowercase();
                }

                // 检查是否为虚拟设备
                let is_virtual = virtual_device_keywords.iter()
                    .any(|keyword| current_adapter.contains(keyword));

                if !is_virtual && line.contains("IPv4 Address") {
                    if let Some(ip_part) = line.split(':').nth(1) {
                        let ip = ip_part.trim();
                        // 过滤掉本地回环地址和特殊地址
                        if !ip.is_empty() && !ip.starts_with("127.") && !ip.starts_with("169.254.") {
                            ips.push(ip.to_string());
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        // macOS: 使用 ifconfig 命令并解析接口信息
        if let Ok(output) = Command::new("ifconfig").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut current_interface = String::new();

            for line in stdout.lines() {
                // 获取接口名称（以数字或: 结尾的行）
                if !line.starts_with('\t') && !line.starts_with(' ') && !line.is_empty() {
                    current_interface = line.split(':').next().unwrap_or("").to_lowercase();
                }

                // 检查是否为虚拟设备
                let is_virtual = virtual_device_keywords.iter()
                    .any(|keyword| current_interface.contains(keyword));

                if !is_virtual && line.contains("inet ") && !line.contains("inet6") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        let ip = parts[1];
                        // 过滤掉本地回环地址和链接本地地址
                        if !ip.starts_with("127.") && !ip.starts_with("169.254.") {
                            ips.push(ip.to_string());
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        // Linux: 使用 ip addr show 命令并解析接口信息
        if let Ok(output) = Command::new("ip")
            .args(&["addr", "show"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut current_interface = String::new();

            for line in stdout.lines() {
                // 获取接口名称（行首的数字和接口名）
                if let Some(interface_name) = line.split_whitespace().nth(1) {
                    if interface_name.contains(':') {
                        current_interface = interface_name.trim_end_matches(':').to_lowercase();
                    }
                }

                // 检查是否为虚拟设备
                let is_virtual = virtual_device_keywords.iter()
                    .any(|keyword| current_interface.contains(keyword));

                if !is_virtual && line.contains("inet ") && !line.contains("inet6") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        let ip_with_mask = parts[1];
                        // 提取 IP 地址（去掉 CIDR 记号）
                        if let Some(ip) = ip_with_mask.split('/').next() {
                            // 过滤掉本地回环地址和链接本地地址
                            if !ip.starts_with("127.") && !ip.starts_with("169.254.") {
                                ips.push(ip.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // 如果通过命令行获取失败，尝试备用方法
    if ips.is_empty() {
        // 备用方案：获取主要的出站 IP
        if let Ok(main_ip) = get_local_ip() {
            ips.push(main_ip);
        }
    }

    // 去重
    ips.sort();
    ips.dedup();

    if ips.is_empty() {
        return Err("未能获取本地 IP 地址".to_string());
    }

    Ok(ips)
}
