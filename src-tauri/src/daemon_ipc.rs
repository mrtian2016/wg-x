// daemon_ipc.rs - 守护进程 IPC 通信模块
// 定义 GUI 和守护进程之间的通信协议

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

// Unix Socket 路径
pub const DAEMON_SOCKET_PATH: &str = "/var/run/wire-vault-daemon.sock";

// IPC 请求
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IpcRequest {
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

// IPC 响应
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IpcResponse {
    pub id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// 隧道配置 (简化版,用于 IPC 传输)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TunnelConfigIpc {
    pub tunnel_id: String,
    pub interface_name: String,
    pub private_key: String,
    pub address: String,
    pub listen_port: Option<u16>,
    pub peers: Vec<PeerConfigIpc>,
    pub wireguard_go_path: String,  // wireguard-go 可执行文件的完整路径
    pub socket_dir: Option<String>, // WireGuard socket 目录 (默认 /var/run/wireguard)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerConfigIpc {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
    pub preshared_key: Option<String>,
}

// 隧道状态
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TunnelStatusIpc {
    pub tunnel_id: String,
    pub status: String, // "running", "stopped"
    pub interface_name: String,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub last_handshake: Option<i64>,
}

// Per-peer 统计信息
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerStatsIpc {
    pub public_key: String,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub last_handshake: Option<i64>,
}

// IPC 客户端 (GUI 使用)
pub struct IpcClient;

impl IpcClient {
    /// 发送请求到守护进程
    pub fn send_request(method: &str, params: serde_json::Value) -> Result<IpcResponse, String> {
        // 连接到守护进程
        let mut stream = UnixStream::connect(DAEMON_SOCKET_PATH)
            .map_err(|e| format!("无法连接到守护进程: {}。请确保守护进程正在运行 (sudo systemctl status wire-vault-daemon)", e))?;

        // 设置读写超时（30秒，足够启动隧道）
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| format!("设置读取超时失败: {}", e))?;
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs(10)))
            .map_err(|e| format!("设置写入超时失败: {}", e))?;

        // 生成请求 ID
        let request_id = uuid::Uuid::new_v4().to_string();

        // 构建请求
        let request = IpcRequest {
            id: request_id.clone(),
            method: method.to_string(),
            params,
        };

        // 序列化请求
        let request_json =
            serde_json::to_string(&request).map_err(|e| format!("序列化请求失败: {}", e))?;

        // 发送请求 (加上换行符作为消息边界)
        stream
            .write_all(format!("{}\n", request_json).as_bytes())
            .map_err(|e| format!("发送请求失败: {}", e))?;

        // 读取响应
        let mut response_data = String::new();
        stream
            .read_to_string(&mut response_data)
            .map_err(|e| format!("读取响应失败（可能超时）: {}", e))?;

        // 解析响应
        let response: IpcResponse =
            serde_json::from_str(&response_data).map_err(|e| format!("解析响应失败: {}", e))?;

        // 检查响应 ID 是否匹配
        if response.id != request_id {
            return Err("响应 ID 不匹配".to_string());
        }

        Ok(response)
    }

    /// 启动隧道
    pub fn start_tunnel(config: TunnelConfigIpc) -> Result<(), String> {
        let params = serde_json::to_value(&config).map_err(|e| format!("序列化配置失败: {}", e))?;

        let response = Self::send_request("start_tunnel", params)?;

        if let Some(error) = response.error {
            return Err(error);
        }

        Ok(())
    }

    /// 停止隧道
    pub fn stop_tunnel(tunnel_id: &str) -> Result<(), String> {
        let params = serde_json::json!({ "tunnel_id": tunnel_id });
        let response = Self::send_request("stop_tunnel", params)?;

        if let Some(error) = response.error {
            return Err(error);
        }

        Ok(())
    }

    /// 获取隧道状态
    pub fn get_tunnel_status(tunnel_id: &str) -> Result<TunnelStatusIpc, String> {
        let params = serde_json::json!({ "tunnel_id": tunnel_id });
        let response = Self::send_request("get_tunnel_status", params)?;

        if let Some(error) = response.error {
            return Err(error);
        }

        let result = response.result.ok_or("响应缺少结果")?;
        let status: TunnelStatusIpc =
            serde_json::from_value(result).map_err(|e| format!("解析状态失败: {}", e))?;

        Ok(status)
    }

    /// 获取隧道的 per-peer 统计信息
    pub fn get_peer_stats(tunnel_id: &str) -> Result<Vec<PeerStatsIpc>, String> {
        let params = serde_json::json!({ "tunnel_id": tunnel_id });
        let response = Self::send_request("get_peer_stats", params)?;

        if let Some(error) = response.error {
            return Err(error);
        }

        let result = response.result.ok_or("响应缺少结果")?;
        let stats: Vec<PeerStatsIpc> =
            serde_json::from_value(result).map_err(|e| format!("解析 peer 统计失败: {}", e))?;

        Ok(stats)
    }

    /// 列出所有运行中的隧道
    pub fn list_tunnels() -> Result<Vec<String>, String> {
        let params = serde_json::json!({});
        let response = Self::send_request("list_tunnels", params)?;

        if let Some(error) = response.error {
            return Err(error);
        }

        let result = response.result.ok_or("响应缺少结果")?;
        let tunnel_ids: Vec<String> =
            serde_json::from_value(result).map_err(|e| format!("解析隧道列表失败: {}", e))?;

        Ok(tunnel_ids)
    }

    /// 心跳检测
    pub fn ping() -> Result<(), String> {
        let params = serde_json::json!({});
        let response = Self::send_request("ping", params)?;

        if let Some(error) = response.error {
            return Err(error);
        }

        Ok(())
    }

    /// 检查守护进程是否正在运行
    pub fn is_daemon_running() -> bool {
        Self::ping().is_ok()
    }
}

// 需要添加 uuid 依赖
// 在 Cargo.toml 中添加: uuid = { version = "1", features = ["v4"] }
