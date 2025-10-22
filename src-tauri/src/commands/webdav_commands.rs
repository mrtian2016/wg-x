use crate::sync::{SyncManager, SyncResult};
use crate::webdav::{LastSyncInfo, WebDavConfig};
use std::fs;
use tauri::{command, AppHandle, Manager};

#[command]
pub fn save_webdav_config(app: AppHandle, config: WebDavConfig) -> Result<(), String> {
    log::info!(
        "保存 WebDAV 配置: enabled={}, url={}",
        config.enabled,
        config.server_url
    );

    let app_data_dir = app.path().app_data_dir().map_err(|e| {
        log::error!("获取应用数据目录失败: {}", e);
        format!("获取应用数据目录失败: {}", e)
    })?;

    fs::create_dir_all(&app_data_dir).map_err(|e| {
        log::error!("创建应用数据目录失败: {}", e);
        format!("创建应用数据目录失败: {}", e)
    })?;

    let config_path = app_data_dir.join("webdav.json");
    let json = serde_json::to_string_pretty(&config).map_err(|e| {
        log::error!("序列化 WebDAV 配置失败: {}", e);
        format!("序列化配置失败: {}", e)
    })?;

    fs::write(&config_path, json).map_err(|e| {
        log::error!("保存 WebDAV 配置失败: {}", e);
        format!("保存配置失败: {}", e)
    })?;

    log::info!("WebDAV 配置保存成功");
    Ok(())
}

#[command]
pub fn load_webdav_config(app: AppHandle) -> Result<WebDavConfig, String> {
    log::info!("加载 WebDAV 配置");

    let app_data_dir = app.path().app_data_dir().map_err(|e| {
        log::error!("获取应用数据目录失败: {}", e);
        format!("获取应用数据目录失败: {}", e)
    })?;

    let config_path = app_data_dir.join("webdav.json");

    if !config_path.exists() {
        log::info!("WebDAV 配置文件不存在，使用默认配置");
        return Ok(WebDavConfig::default());
    }

    let content = fs::read_to_string(&config_path).map_err(|e| {
        log::error!("读取 WebDAV 配置失败: {}", e);
        format!("读取配置失败: {}", e)
    })?;

    let config: WebDavConfig = serde_json::from_str(&content).map_err(|e| {
        log::error!("解析 WebDAV 配置失败: {}", e);
        format!("解析配置失败: {}", e)
    })?;

    log::info!("WebDAV 配置加载成功: enabled={}", config.enabled);
    Ok(config)
}

#[command]
pub async fn test_webdav_connection(config: WebDavConfig) -> Result<(), String> {
    let client = crate::webdav::WebDavClient::new(config)?;
    client.test_connection().await
}

#[command]
pub async fn sync_to_webdav(app: AppHandle) -> Result<SyncResult, String> {
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

#[command]
pub async fn sync_from_webdav(app: AppHandle) -> Result<SyncResult, String> {
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

#[command]
pub async fn sync_bidirectional_webdav(app: AppHandle) -> Result<SyncResult, String> {
    log::info!("开始双向 WebDAV 同步");

    let app_data_dir = app.path().app_data_dir().map_err(|e| {
        log::error!("获取应用数据目录失败: {}", e);
        format!("获取应用数据目录失败: {}", e)
    })?;

    let config = load_webdav_config(app.clone())?;

    if !config.enabled {
        log::warn!("WebDAV 同步未启用");
        return Err("WebDAV 同步未启用".to_string());
    }

    let manager = SyncManager::new(app_data_dir);
    manager.init_client(config).await?;
    let result = manager.sync_bidirectional().await?;

    log::info!(
        "双向同步完成: 服务端上传={}, 服务端下载={}, 历史上传={}, 历史下载={}",
        result.servers_uploaded,
        result.servers_downloaded,
        result.history_uploaded,
        result.history_downloaded
    );

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

    if let Err(e) = save_last_sync_info(app.clone(), sync_info) {
        log::error!("保存同步信息失败: {}", e);
    }

    Ok(result)
}

#[command]
pub fn save_last_sync_info(app: AppHandle, info: LastSyncInfo) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    fs::create_dir_all(&app_data_dir).map_err(|e| format!("创建应用数据目录失败: {}", e))?;

    let sync_info_path = app_data_dir.join("last_sync.json");
    let json =
        serde_json::to_string_pretty(&info).map_err(|e| format!("序列化同步信息失败: {}", e))?;

    fs::write(&sync_info_path, json).map_err(|e| format!("保存同步信息失败: {}", e))?;

    Ok(())
}

#[command]
pub fn load_last_sync_info(app: AppHandle) -> Result<Option<LastSyncInfo>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let sync_info_path = app_data_dir.join("last_sync.json");

    if !sync_info_path.exists() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(&sync_info_path).map_err(|e| format!("读取同步信息失败: {}", e))?;

    let info: LastSyncInfo =
        serde_json::from_str(&content).map_err(|e| format!("解析同步信息失败: {}", e))?;

    Ok(Some(info))
}
