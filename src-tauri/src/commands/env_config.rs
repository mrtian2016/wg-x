use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tauri::command;

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

#[command]
pub fn load_env_config(work_dir: String) -> Result<EnvConfig, String> {
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
