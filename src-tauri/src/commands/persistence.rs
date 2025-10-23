use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{command, AppHandle, Manager};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PersistentConfig {
    pub peer_public_key: String,
    pub preshared_key: String,
    pub endpoint: String,
    pub allowed_ips: String,
    pub persistent_keepalive: String,
    pub peer_interface: String,
    pub next_peer_id: u32,
}

#[command]
pub fn save_persistent_config(app: AppHandle, config: PersistentConfig) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    fs::create_dir_all(&app_data_dir).map_err(|e| format!("创建应用数据目录失败: {}", e))?;

    let config_path = app_data_dir.join("config.json");
    let json =
        serde_json::to_string_pretty(&config).map_err(|e| format!("序列化配置失败: {}", e))?;

    fs::write(&config_path, json).map_err(|e| format!("保存配置失败: {}", e))?;

    Ok(())
}

#[command]
pub fn load_persistent_config(app: AppHandle) -> Result<PersistentConfig, String> {
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

#[command]
pub fn get_next_peer_id(app: AppHandle) -> Result<u32, String> {
    let config = load_persistent_config(app)?;

    if config.next_peer_id == 0 {
        Ok(1)
    } else {
        Ok(config.next_peer_id)
    }
}

#[command]
pub fn clear_cached_config(app: AppHandle) -> Result<(), String> {
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
