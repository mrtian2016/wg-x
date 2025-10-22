use crate::commands::persistence::PersistentConfig;
use crate::sync::SyncManager;
use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{command, AppHandle, Manager};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub peer_public_key: String,
    pub preshared_key: String,
    pub endpoint: String,
    pub allowed_ips: String,
    pub persistent_keepalive: String,
    pub ikuai_interface: String,
    pub next_peer_id: u32,
    pub created_at: i64,
}

#[command]
pub fn save_server_config(app: AppHandle, config: ServerConfig) -> Result<(), String> {
    log::info!("保存服务端配置: id={}, name={}", config.id, config.name);

    let app_data_dir = app.path().app_data_dir().map_err(|e| {
        log::error!("获取应用数据目录失败: {}", e);
        format!("获取应用数据目录失败: {}", e)
    })?;

    let servers_dir = app_data_dir.join("servers");
    fs::create_dir_all(&servers_dir).map_err(|e| {
        log::error!("创建服务端目录失败: {}", e);
        format!("创建服务端目录失败: {}", e)
    })?;

    let file_path = servers_dir.join(format!("{}.json", config.id));
    let json = serde_json::to_string_pretty(&config).map_err(|e| {
        log::error!("序列化服务端配置失败: {}", e);
        format!("序列化服务端配置失败: {}", e)
    })?;

    fs::write(&file_path, json).map_err(|e| {
        log::error!("保存服务端配置失败: {}", e);
        format!("保存服务端配置失败: {}", e)
    })?;

    log::info!("服务端配置保存成功: {}", config.id);
    Ok(())
}

#[command]
pub fn get_server_list(app: AppHandle) -> Result<Vec<ServerConfig>, String> {
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

    servers.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(servers)
}

#[command]
pub fn get_server_detail(app: AppHandle, id: String) -> Result<ServerConfig, String> {
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

#[command]
pub async fn delete_server(app: AppHandle, id: String) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let filename = format!("{}.json", id);
    let file_path = app_data_dir.join("servers").join(&filename);

    if file_path.exists() {
        fs::remove_file(&file_path).map_err(|e| format!("删除服务端配置失败: {}", e))?;

        let manager = SyncManager::new(app_data_dir);
        if let Err(e) = manager.record_deletion("servers", &filename).await {
            log::error!("记录删除操作失败: {}", e);
        }
    }

    Ok(())
}

#[command]
pub async fn clear_all_servers(app: AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let servers_dir = app_data_dir.join("servers");

    if servers_dir.exists() {
        let manager = SyncManager::new(app_data_dir.clone());

        if let Ok(entries) = fs::read_dir(&servers_dir) {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.ends_with(".json") {
                        if let Err(e) = manager.record_deletion("servers", filename).await {
                            log::error!("记录删除操作失败: {}", e);
                        }
                    }
                }
            }
        }

        fs::remove_dir_all(&servers_dir).map_err(|e| format!("清空服务端配置失败: {}", e))?;
    }

    Ok(())
}

#[command]
pub fn get_next_peer_id_for_server(app: AppHandle, server_id: String) -> Result<u32, String> {
    let server = get_server_detail(app, server_id)?;

    if server.next_peer_id == 0 {
        Ok(1)
    } else {
        Ok(server.next_peer_id)
    }
}

#[command]
pub fn update_server_peer_id(
    app: AppHandle,
    server_id: String,
    next_peer_id: u32,
) -> Result<(), String> {
    let mut server = get_server_detail(app.clone(), server_id)?;
    server.next_peer_id = next_peer_id;
    save_server_config(app, server)?;
    Ok(())
}

#[command]
pub fn migrate_old_config_to_server(app: AppHandle) -> Result<Option<String>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let old_config_path = app_data_dir.join("config.json");

    if !old_config_path.exists() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(&old_config_path).map_err(|e| format!("读取旧配置失败: {}", e))?;

    let old_config: PersistentConfig =
        serde_json::from_str(&content).map_err(|e| format!("解析旧配置失败: {}", e))?;

    if old_config.peer_public_key.is_empty() || old_config.endpoint.is_empty() {
        return Ok(None);
    }

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

    save_server_config(app.clone(), server_config)?;

    fs::rename(&old_config_path, app_data_dir.join("config.json.bak"))
        .map_err(|e| format!("备份旧配置失败: {}", e))?;

    Ok(Some(server_id))
}
