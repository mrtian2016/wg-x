use crate::webdav::{WebDavClient, WebDavConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 删除追踪记录
#[derive(Serialize, Deserialize, Default)]
struct DeletedFiles {
    servers: HashSet<String>,
    history: HashSet<String>,
}

/// 同步管理器
pub struct SyncManager {
    client: Arc<Mutex<Option<WebDavClient>>>,
    app_data_dir: PathBuf,
}

impl SyncManager {
    /// 创建新的同步管理器
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
            app_data_dir,
        }
    }

    /// 初始化 WebDAV 客户端
    pub async fn init_client(&self, config: WebDavConfig) -> Result<(), String> {
        if !config.enabled {
            *self.client.lock().await = None;
            return Ok(());
        }

        let client = WebDavClient::new(config)?;
        *self.client.lock().await = Some(client);
        Ok(())
    }

    /// 测试连接
    #[allow(dead_code)]
    pub async fn test_connection(&self) -> Result<(), String> {
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .ok_or_else(|| "WebDAV 未配置".to_string())?;

        client.test_connection().await
    }

    /// 同步所有数据到 WebDAV
    pub async fn sync_to_remote(&self) -> Result<SyncResult, String> {
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .ok_or_else(|| "WebDAV 未配置".to_string())?;

        let mut result = SyncResult::default();

        // 确保远程目录存在
        client.create_directory("servers").await?;
        client.create_directory("history").await?;

        // 同步服务端配置
        let servers_dir = self.app_data_dir.join("servers");
        if servers_dir.exists() {
            result.servers_uploaded += self
                .sync_directory_to_remote(&client, &servers_dir, "servers")
                .await?;
        }

        // 同步历史记录
        let history_dir = self.app_data_dir.join("history");
        if history_dir.exists() {
            result.history_uploaded += self
                .sync_directory_to_remote(&client, &history_dir, "history")
                .await?;
        }

        Ok(result)
    }

    /// 从 WebDAV 同步数据到本地
    pub async fn sync_from_remote(&self) -> Result<SyncResult, String> {
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .ok_or_else(|| "WebDAV 未配置".to_string())?;

        let mut result = SyncResult::default();

        // 同步服务端配置
        let servers_dir = self.app_data_dir.join("servers");
        tokio::fs::create_dir_all(&servers_dir)
            .await
            .map_err(|e| format!("创建 servers 目录失败: {}", e))?;

        result.servers_downloaded += self
            .sync_directory_from_remote(&client, "servers", &servers_dir)
            .await?;

        // 同步历史记录
        let history_dir = self.app_data_dir.join("history");
        tokio::fs::create_dir_all(&history_dir)
            .await
            .map_err(|e| format!("创建 history 目录失败: {}", e))?;

        result.history_downloaded += self
            .sync_directory_from_remote(&client, "history", &history_dir)
            .await?;

        Ok(result)
    }

    /// 双向同步（智能合并）
    pub async fn sync_bidirectional(&self) -> Result<SyncResult, String> {
        let client_guard = self.client.lock().await;
        let client = client_guard
            .as_ref()
            .ok_or_else(|| "WebDAV 未配置".to_string())?;

        let mut result = SyncResult::default();

        // 确保远程目录存在
        client.create_directory("servers").await?;
        client.create_directory("history").await?;

        // 双向同步服务端配置
        let servers_dir = self.app_data_dir.join("servers");
        tokio::fs::create_dir_all(&servers_dir)
            .await
            .map_err(|e| format!("创建 servers 目录失败: {}", e))?;

        let (uploaded, downloaded) = self
            .sync_directory_bidirectional(&client, &servers_dir, "servers")
            .await?;
        result.servers_uploaded += uploaded;
        result.servers_downloaded += downloaded;

        // 双向同步历史记录
        let history_dir = self.app_data_dir.join("history");
        tokio::fs::create_dir_all(&history_dir)
            .await
            .map_err(|e| format!("创建 history 目录失败: {}", e))?;

        let (uploaded, downloaded) = self
            .sync_directory_bidirectional(&client, &history_dir, "history")
            .await?;
        result.history_uploaded += uploaded;
        result.history_downloaded += downloaded;

        // 同步完成后清除删除记录
        drop(client_guard); // 释放锁
        if let Err(e) = self.clear_deletion_records().await {
            eprintln!("清除删除记录失败: {}", e);
        }

        Ok(result)
    }

    /// 记录文件删除操作
    pub async fn record_deletion(&self, file_type: &str, filename: &str) -> Result<(), String> {
        let mut deleted = self.load_deleted_files().await?;

        match file_type {
            "servers" => {
                deleted.servers.insert(filename.to_string());
            }
            "history" => {
                deleted.history.insert(filename.to_string());
            }
            _ => return Err(format!("未知的文件类型: {}", file_type)),
        }

        self.save_deleted_files(&deleted).await?;
        Ok(())
    }

    /// 清除删除记录（同步完成后调用）
    async fn clear_deletion_records(&self) -> Result<(), String> {
        let deleted_file = self.app_data_dir.join(".deleted_files.json");
        if deleted_file.exists() {
            tokio::fs::remove_file(&deleted_file)
                .await
                .map_err(|e| format!("清除删除记录失败: {}", e))?;
        }
        Ok(())
    }

    // === 私有辅助方法 ===

    /// 加载删除追踪文件
    async fn load_deleted_files(&self) -> Result<DeletedFiles, String> {
        let deleted_file = self.app_data_dir.join(".deleted_files.json");

        if !deleted_file.exists() {
            return Ok(DeletedFiles::default());
        }

        let content = tokio::fs::read_to_string(&deleted_file)
            .await
            .map_err(|e| format!("读取删除记录失败: {}", e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("解析删除记录失败: {}", e))
    }

    /// 保存删除追踪文件
    async fn save_deleted_files(&self, deleted: &DeletedFiles) -> Result<(), String> {
        let deleted_file = self.app_data_dir.join(".deleted_files.json");

        let content = serde_json::to_string_pretty(deleted)
            .map_err(|e| format!("序列化删除记录失败: {}", e))?;

        tokio::fs::write(&deleted_file, content)
            .await
            .map_err(|e| format!("保存删除记录失败: {}", e))?;

        Ok(())
    }

    /// 同步目录到远程
    async fn sync_directory_to_remote(
        &self,
        client: &WebDavClient,
        local_dir: &Path,
        remote_dir: &str,
    ) -> Result<usize, String> {
        let mut count = 0;

        let mut entries = tokio::fs::read_dir(local_dir)
            .await
            .map_err(|e| format!("读取目录失败: {}", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("读取目录项失败: {}", e))?
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    let remote_path = format!("{}/{}", remote_dir, filename);
                    client.upload_file(&path, &remote_path).await?;
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// 从远程同步目录
    async fn sync_directory_from_remote(
        &self,
        client: &WebDavClient,
        remote_dir: &str,
        local_dir: &Path,
    ) -> Result<usize, String> {
        let mut count = 0;

        // 列出远程文件
        let files = match client.list_directory(remote_dir).await {
            Ok(files) => files,
            Err(_) => {
                // 远程目录不存在，创建它
                client.create_directory(remote_dir).await?;
                return Ok(0);
            }
        };

        for filename in files {
            if filename.ends_with(".json") {
                let remote_path = format!("{}/{}", remote_dir, filename);
                let local_path = local_dir.join(&filename);
                client.download_file(&remote_path, &local_path).await?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// 双向同步目录（基于时间戳，支持删除同步）
    async fn sync_directory_bidirectional(
        &self,
        client: &WebDavClient,
        local_dir: &Path,
        remote_dir: &str,
    ) -> Result<(usize, usize), String> {
        let mut uploaded = 0;
        let mut downloaded = 0;

        // 加载删除记录
        let deleted = self.load_deleted_files().await?;
        let deleted_set = match remote_dir {
            "servers" => &deleted.servers,
            "history" => &deleted.history,
            _ => &HashSet::new(),
        };

        // 获取本地文件列表
        let mut local_files = std::collections::HashMap::new();
        let mut entries = tokio::fs::read_dir(local_dir)
            .await
            .map_err(|e| format!("读取本地目录失败: {}", e))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("读取目录项失败: {}", e))?
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if filename.ends_with(".json") {
                        if let Ok(metadata) = tokio::fs::metadata(&path).await {
                            if let Ok(modified) = metadata.modified() {
                                if let Ok(timestamp) = modified.duration_since(std::time::UNIX_EPOCH)
                                {
                                    local_files
                                        .insert(filename.to_string(), timestamp.as_secs() as i64);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 获取远程文件列表
        let remote_files = match client.list_directory(remote_dir).await {
            Ok(files) => files,
            Err(_) => {
                // 远程目录不存在，上传所有本地文件
                for filename in local_files.keys() {
                    let local_path = local_dir.join(filename);
                    let remote_path = format!("{}/{}", remote_dir, filename);
                    client.upload_file(&local_path, &remote_path).await?;
                    uploaded += 1;
                }
                return Ok((uploaded, downloaded));
            }
        };

        // 处理每个远程文件
        for filename in &remote_files {
            if !filename.ends_with(".json") {
                continue;
            }

            let remote_path = format!("{}/{}", remote_dir, filename);
            let local_path = local_dir.join(filename);

            // 检查是否在删除列表中
            if deleted_set.contains(filename) {
                // 这个文件已被本地删除，同步删除到远程
                println!("同步删除远程文件: {}", filename);
                if let Err(e) = client.delete_file(&remote_path).await {
                    eprintln!("删除远程文件失败: {}", e);
                }
                continue;
            }

            // 获取远程文件的修改时间
            let remote_modified = client.get_last_modified(&remote_path).await?;

            if local_files.contains_key(filename) {
                // 本地和远程都存在，比较时间戳
                let local_modified = local_files[filename];

                if let Some(remote_time) = remote_modified {
                    if remote_time > local_modified {
                        // 远程更新，下载
                        client.download_file(&remote_path, &local_path).await?;
                        downloaded += 1;
                    } else if local_modified > remote_time {
                        // 本地更新，上传
                        client.upload_file(&local_path, &remote_path).await?;
                        uploaded += 1;
                    }
                    // 如果时间相同，不做任何操作
                }

                // 从列表中移除已处理的文件
                local_files.remove(filename);
            } else {
                // 仅远程存在，下载
                client.download_file(&remote_path, &local_path).await?;
                downloaded += 1;
            }
        }

        // 处理仅本地存在的文件，上传
        for filename in local_files.keys() {
            let local_path = local_dir.join(filename);
            let remote_path = format!("{}/{}", remote_dir, filename);
            client.upload_file(&local_path, &remote_path).await?;
            uploaded += 1;
        }

        Ok((uploaded, downloaded))
    }
}

/// 同步结果
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncResult {
    pub servers_uploaded: usize,
    pub servers_downloaded: usize,
    pub history_uploaded: usize,
    pub history_downloaded: usize,
}

impl SyncResult {
    #[allow(dead_code)]
    pub fn total_uploaded(&self) -> usize {
        self.servers_uploaded + self.history_uploaded
    }

    #[allow(dead_code)]
    pub fn total_downloaded(&self) -> usize {
        self.servers_downloaded + self.history_downloaded
    }
}
