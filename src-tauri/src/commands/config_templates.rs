use crate::commands::key_management::compute_public_key;
use serde::{Deserialize, Serialize};
use tauri::command;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WgConfig {
    pub interface_name: String,
    pub private_key: String,
    pub address: String,
    pub listen_port: Option<String>,
    pub dns: Option<String>,
    pub peer_public_key: String,
    pub preshared_key: Option<String>,
    pub endpoint: String,
    pub allowed_ips: String,
    pub persistent_keepalive: Option<String>,
    pub ikuai_id: u32,
    pub ikuai_interface: String,
    pub ikuai_comment: String,
}

#[command]
pub fn generate_wg_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
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

#[command]
pub fn generate_ikuai_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
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

#[command]
pub fn generate_surge_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
    let self_ip = config.address.split('/').next().unwrap_or(&config.address);

    let section_name = config.interface_name.replace(" ", "");

    let mut surge_config = String::new();

    surge_config.push_str("[Proxy]\n");
    surge_config.push_str(&format!(
        "wireguard-{} = wireguard, section-name = {}\n\n",
        section_name, section_name
    ));

    surge_config.push_str(&format!("[WireGuard {}]\n", section_name));
    surge_config.push_str(&format!("private-key = {}\n", config.private_key));
    surge_config.push_str(&format!("self-ip = {}\n", self_ip));
    surge_config.push_str("mtu = 1280\n");

    let mut peer_config = format!("peer = (public-key = {}", config.peer_public_key);

    peer_config.push_str(&format!(", allowed-ips = \"{}\"", config.allowed_ips));
    peer_config.push_str(&format!(", endpoint = {}", config.endpoint));

    if let Some(psk) = &config.preshared_key {
        if !psk.is_empty() {
            peer_config.push_str(&format!(", preshared-key = {}", psk));
        }
    }

    if let Some(keepalive) = &config.persistent_keepalive {
        if !keepalive.is_empty() {
            peer_config.push_str(&format!(", keepalive = {}", keepalive));
        }
    }

    peer_config.push_str(")\n");
    surge_config.push_str(&peer_config);

    Ok(surge_config)
}

#[command]
pub fn generate_mikrotik_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
    let public_key = compute_public_key(&config.private_key)?;

    let allowed_address = &config.address;

    let mut command = format!(
        "/interface/wireguard/peers/add \\\n  interface={} \\\n  public-key=\"{}\" \\\n  allowed-address={} \\\n  comment=\"{}\"",
        config.ikuai_interface,
        public_key,
        allowed_address,
        config.ikuai_comment
    );

    if let Some(psk) = &config.preshared_key {
        if !psk.is_empty() {
            command.push_str(&format!(" \\\n  preshared-key=\"{}\"", psk));
        }
    }

    Ok(command)
}

#[command]
pub fn generate_openwrt_config(config: WgConfig, _work_dir: String) -> Result<String, String> {
    let public_key = compute_public_key(&config.private_key)?;

    let section_name = format!("wireguard_{}", config.interface_name);

    let mut commands = String::new();

    commands.push_str(&format!("uci add network {}\n", section_name));
    commands.push_str(&format!(
        "uci set network.@{}[-1].public_key='{}'\n",
        section_name, public_key
    ));

    if let Some(psk) = &config.preshared_key {
        if !psk.is_empty() {
            commands.push_str(&format!(
                "uci set network.@{}[-1].preshared_key='{}'\n",
                section_name, psk
            ));
        }
    }

    commands.push_str(&format!(
        "uci set network.@{}[-1].allowed_ips='{}'\n",
        section_name, config.address
    ));

    if let Some(keepalive) = &config.persistent_keepalive {
        if !keepalive.is_empty() {
            commands.push_str(&format!(
                "uci set network.@{}[-1].persistent_keepalive='{}'\n",
                section_name, keepalive
            ));
        }
    }

    commands.push_str(&format!(
        "uci set network.@{}[-1].description='{}'\n",
        section_name, config.ikuai_comment
    ));

    commands.push_str("# 提交配置\n");
    commands.push_str("uci commit network\n");
    commands.push_str(&format!("ifup {}", config.interface_name));

    Ok(commands)
}
