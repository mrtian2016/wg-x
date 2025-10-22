use crate::sync::SyncManager;
use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{command, AppHandle, Manager};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub interface_name: String,
    pub ikuai_comment: String,
    pub ikuai_id: u32,
    pub address: String,
    pub wg_config: String,
    pub ikuai_config: String,
    pub surge_config: Option<String>,
    pub mikrotik_config: Option<String>,
    pub openwrt_config: Option<String>,
    pub public_key: String,
    pub server_id: String,
    pub server_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HistoryListItem {
    pub id: String,
    pub timestamp: i64,
    pub interface_name: String,
    pub ikuai_comment: String,
    pub ikuai_id: u32,
    pub address: String,
    pub public_key: String,
    pub server_id: String,
    pub server_name: String,
}

#[command]
pub fn save_to_history(app: AppHandle, entry: HistoryEntry) -> Result<(), String> {
    log::info!(
        "保存历史记录: id={}, interface_name={}",
        entry.id,
        entry.interface_name
    );

    let app_data_dir = app.path().app_data_dir().map_err(|e| {
        log::error!("获取应用数据目录失败: {}", e);
        format!("获取应用数据目录失败: {}", e)
    })?;

    let history_dir = app_data_dir.join("history");
    fs::create_dir_all(&history_dir).map_err(|e| {
        log::error!("创建历史目录失败: {}", e);
        format!("创建历史目录失败: {}", e)
    })?;

    let file_path = history_dir.join(format!("{}.json", entry.id));
    let json = serde_json::to_string_pretty(&entry).map_err(|e| {
        log::error!("序列化历史记录失败: {}", e);
        format!("序列化历史记录失败: {}", e)
    })?;

    fs::write(&file_path, json).map_err(|e| {
        log::error!("保存历史记录失败: {}", e);
        format!("保存历史记录失败: {}", e)
    })?;

    log::info!("历史记录保存成功: {}", entry.id);
    Ok(())
}

#[command]
pub fn get_history_list(app: AppHandle) -> Result<Vec<HistoryListItem>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let history_dir = app_data_dir.join("history");

    if !history_dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    let entries = fs::read_dir(&history_dir).map_err(|e| format!("读取历史目录失败: {}", e))?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(history_entry) = serde_json::from_str::<HistoryEntry>(&content) {
                        items.push(HistoryListItem {
                            id: history_entry.id,
                            timestamp: history_entry.timestamp,
                            interface_name: history_entry.interface_name,
                            ikuai_comment: history_entry.ikuai_comment,
                            ikuai_id: history_entry.ikuai_id,
                            address: history_entry.address,
                            public_key: history_entry.public_key,
                            server_id: history_entry.server_id,
                            server_name: history_entry.server_name,
                        });
                    }
                }
            }
        }
    }

    items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(items)
}

#[command]
pub fn get_history_detail(app: AppHandle, id: String) -> Result<HistoryEntry, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let file_path = app_data_dir.join("history").join(format!("{}.json", id));

    if !file_path.exists() {
        return Err("历史记录不存在".to_string());
    }

    let content = fs::read_to_string(&file_path).map_err(|e| format!("读取历史记录失败: {}", e))?;

    let entry: HistoryEntry =
        serde_json::from_str(&content).map_err(|e| format!("解析历史记录失败: {}", e))?;

    Ok(entry)
}

#[command]
pub async fn delete_history(app: AppHandle, id: String) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let filename = format!("{}.json", id);
    let file_path = app_data_dir.join("history").join(&filename);

    if file_path.exists() {
        fs::remove_file(&file_path).map_err(|e| format!("删除历史记录失败: {}", e))?;

        let manager = SyncManager::new(app_data_dir);
        if let Err(e) = manager.record_deletion("history", &filename).await {
            log::error!("记录删除操作失败: {}", e);
        }
    }

    Ok(())
}

#[command]
pub async fn clear_all_history(app: AppHandle) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let history_dir = app_data_dir.join("history");

    if history_dir.exists() {
        let manager = SyncManager::new(app_data_dir.clone());

        if let Ok(entries) = fs::read_dir(&history_dir) {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.ends_with(".json") {
                        if let Err(e) = manager.record_deletion("history", filename).await {
                            log::error!("记录删除操作失败: {}", e);
                        }
                    }
                }
            }
        }

        fs::remove_dir_all(&history_dir).map_err(|e| format!("清空历史记录失败: {}", e))?;
    }

    Ok(())
}

#[command]
pub fn export_all_configs_zip(app: AppHandle, zip_path: String) -> Result<(), String> {
    use std::io::Write;
    use zip::write::FileOptions;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let history_dir = app_data_dir.join("history");

    if !history_dir.exists() {
        return Err("没有历史记录可导出".to_string());
    }

    let file = std::fs::File::create(&zip_path).map_err(|e| format!("创建 ZIP 文件失败: {}", e))?;

    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    let entries = fs::read_dir(&history_dir).map_err(|e| format!("读取历史目录失败: {}", e))?;

    let mut all_peers = Vec::new();
    let mut config_count = 0;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(history_entry) = serde_json::from_str::<HistoryEntry>(&content) {
                        let base_name = format!(
                            "{}-{}",
                            history_entry.ikuai_comment.replace(" ", "_"),
                            history_entry.ikuai_id
                        );

                        let wg_filename = format!("{}.conf", base_name);
                        zip.start_file(&wg_filename, options)
                            .map_err(|e| format!("添加文件到 ZIP 失败: {}", e))?;
                        zip.write_all(history_entry.wg_config.as_bytes())
                            .map_err(|e| format!("写入文件到 ZIP 失败: {}", e))?;

                        if let Some(surge_config) = &history_entry.surge_config {
                            let surge_filename = format!("{}_surge.conf", base_name);
                            zip.start_file(&surge_filename, options)
                                .map_err(|e| format!("添加 Surge 文件到 ZIP 失败: {}", e))?;
                            zip.write_all(surge_config.as_bytes())
                                .map_err(|e| format!("写入 Surge 文件到 ZIP 失败: {}", e))?;
                        }

                        all_peers.push(history_entry.ikuai_config);
                        config_count += 1;
                    }
                }
            }
        }
    }

    if config_count == 0 {
        return Err("没有找到有效的配置".to_string());
    }

    let all_peers_content = all_peers.join("\n");
    zip.start_file("all_peers.txt", options)
        .map_err(|e| format!("添加 all_peers.txt 到 ZIP 失败: {}", e))?;
    zip.write_all(all_peers_content.as_bytes())
        .map_err(|e| format!("写入 all_peers.txt 到 ZIP 失败: {}", e))?;

    zip.finish()
        .map_err(|e| format!("完成 ZIP 文件失败: {}", e))?;

    Ok(())
}

#[command]
pub fn get_history_list_by_server(
    app: AppHandle,
    server_id: String,
) -> Result<Vec<HistoryListItem>, String> {
    let all_history = get_history_list(app)?;

    let filtered: Vec<HistoryListItem> = all_history
        .into_iter()
        .filter(|item| item.server_id == server_id)
        .collect();

    Ok(filtered)
}
