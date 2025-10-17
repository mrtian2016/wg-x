use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::path::Path;
use url::Url;

/// WebDAV 配置结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebDavConfig {
    pub enabled: bool,
    pub server_url: String,
    pub username: String,
    pub password: String,
    pub sync_interval: u64, // 同步间隔(秒)
    #[serde(default)]
    pub auto_sync_enabled: bool, // 自动同步开关
}

/// 最后同步信息
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LastSyncInfo {
    pub timestamp: i64,                  // 同步时间戳
    pub sync_type: String,               // 同步类型: "bidirectional", "upload", "download", "auto"
    pub servers_uploaded: usize,         // 上传的服务端配置数量
    pub servers_downloaded: usize,       // 下载的服务端配置数量
    pub history_uploaded: usize,         // 上传的历史记录数量
    pub history_downloaded: usize,       // 下载的历史记录数量
}

impl Default for WebDavConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: String::new(),
            username: String::new(),
            password: String::new(),
            sync_interval: 300, // 默认 5 分钟
            auto_sync_enabled: false, // 默认关闭自动同步
        }
    }
}

/// WebDAV 客户端
pub struct WebDavClient {
    client: Client,
    config: WebDavConfig,
}

impl WebDavClient {
    /// 创建新的 WebDAV 客户端
    pub fn new(config: WebDavConfig) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

        Ok(Self { client, config })
    }

    /// 测试连接
    pub async fn test_connection(&self) -> Result<(), String> {
        let url = self.normalize_url(&self.config.server_url)?;

        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Depth", "0")
            .send()
            .await
            .map_err(|e| format!("连接失败: {}", e))?;

        if response.status().is_success() || response.status() == StatusCode::MULTI_STATUS {
            Ok(())
        } else {
            Err(format!("WebDAV 服务器响应错误: {}", response.status()))
        }
    }

    /// 上传文件
    pub async fn upload_file(&self, local_path: &Path, remote_path: &str) -> Result<(), String> {
        let content = tokio::fs::read(local_path)
            .await
            .map_err(|e| format!("读取本地文件失败: {}", e))?;

        let url = self.build_url(remote_path)?;

        // 确保远程目录存在
        if let Some(parent) = Path::new(remote_path).parent() {
            if parent != Path::new("") {
                self.create_directory(parent.to_str().unwrap_or(""))
                    .await?;
            }
        }

        let response = self
            .client
            .put(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .body(content)
            .send()
            .await
            .map_err(|e| format!("上传文件失败: {}", e))?;

        if response.status().is_success() || response.status() == StatusCode::CREATED {
            Ok(())
        } else {
            Err(format!("上传文件失败: {}", response.status()))
        }
    }

    /// 下载文件
    pub async fn download_file(&self, remote_path: &str, local_path: &Path) -> Result<(), String> {
        let url = self.build_url(remote_path)?;

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| format!("下载文件失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("下载文件失败: {}", response.status()));
        }

        let content = response
            .bytes()
            .await
            .map_err(|e| format!("读取响应内容失败: {}", e))?;

        // 确保本地目录存在
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("创建本地目录失败: {}", e))?;
        }

        tokio::fs::write(local_path, content)
            .await
            .map_err(|e| format!("保存文件失败: {}", e))?;

        Ok(())
    }

    /// 删除文件
    pub async fn delete_file(&self, remote_path: &str) -> Result<(), String> {
        let url = self.build_url(remote_path)?;

        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| format!("删除文件失败: {}", e))?;

        if response.status().is_success() || response.status() == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(format!("删除文件失败: {}", response.status()))
        }
    }

    /// 创建目录
    pub async fn create_directory(&self, remote_path: &str) -> Result<(), String> {
        let url = self.build_url(&format!("{}/", remote_path.trim_end_matches('/')))?;

        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), &url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| format!("创建目录失败: {}", e))?;

        if response.status().is_success()
            || response.status() == StatusCode::CREATED
            || response.status() == StatusCode::METHOD_NOT_ALLOWED
        {
            // METHOD_NOT_ALLOWED 表示目录已存在
            Ok(())
        } else {
            Err(format!("创建目录失败: {}", response.status()))
        }
    }

    /// 列出目录内容
    pub async fn list_directory(&self, remote_path: &str) -> Result<Vec<String>, String> {
        let url = self.build_url(&format!("{}/", remote_path.trim_end_matches('/')))?;

        let propfind_body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
    <d:prop>
        <d:displayname/>
        <d:getcontentlength/>
        <d:getlastmodified/>
        <d:resourcetype/>
    </d:prop>
</d:propfind>"#;

        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body(propfind_body)
            .send()
            .await
            .map_err(|e| format!("列出目录失败: {}", e))?;

        if response.status() != StatusCode::MULTI_STATUS {
            return Err(format!("列出目录失败: {}", response.status()));
        }

        let body = response
            .text()
            .await
            .map_err(|e| format!("读取响应失败: {}", e))?;

        // 解析 XML 响应
        self.parse_propfind_response(&body, remote_path)
    }

    /// 检查文件是否存在
    #[allow(dead_code)]
    pub async fn file_exists(&self, remote_path: &str) -> Result<bool, String> {
        let url = self.build_url(remote_path)?;

        let response = self
            .client
            .head(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| format!("检查文件失败: {}", e))?;

        Ok(response.status().is_success())
    }

    /// 获取文件修改时间
    pub async fn get_last_modified(&self, remote_path: &str) -> Result<Option<i64>, String> {
        let url = self.build_url(remote_path)?;

        let propfind_body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
    <d:prop>
        <d:getlastmodified/>
    </d:prop>
</d:propfind>"#;

        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Depth", "0")
            .header("Content-Type", "application/xml")
            .body(propfind_body)
            .send()
            .await
            .map_err(|e| format!("获取文件信息失败: {}", e))?;

        if response.status() != StatusCode::MULTI_STATUS {
            return Ok(None);
        }

        let body = response
            .text()
            .await
            .map_err(|e| format!("读取响应失败: {}", e))?;

        self.parse_last_modified(&body)
    }

    // === 辅助方法 ===

    /// 标准化 URL
    fn normalize_url(&self, url_str: &str) -> Result<String, String> {
        let url = Url::parse(url_str).map_err(|e| format!("无效的 URL: {}", e))?;
        Ok(url.to_string())
    }

    /// 构建完整的 WebDAV URL
    fn build_url(&self, path: &str) -> Result<String, String> {
        // 确保基础 URL 以 / 结尾，这样 join 才能正确拼接
        let base_url = if self.config.server_url.ends_with('/') {
            self.config.server_url.clone()
        } else {
            format!("{}/", self.config.server_url)
        };

        let base = Url::parse(&base_url)
            .map_err(|e| format!("无效的服务器 URL: {}", e))?;

        let path = path.trim_start_matches('/');
        let url = base
            .join(path)
            .map_err(|e| format!("构建 URL 失败: {}", e))?;

        Ok(url.to_string())
    }

    /// 解析 PROPFIND 响应
    fn parse_propfind_response(&self, xml: &str, base_path: &str) -> Result<Vec<String>, String> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut files = Vec::new();
        let mut current_href = String::new();
        let mut in_href = false;

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    if e.name().as_ref() == b"d:href" || e.name().as_ref() == b"D:href" {
                        in_href = true;
                        current_href.clear();
                    }
                }
                Ok(Event::Text(e)) => {
                    if in_href {
                        current_href.push_str(&String::from_utf8_lossy(&e));
                    }
                }
                Ok(Event::End(e)) => {
                    if e.name().as_ref() == b"d:href" || e.name().as_ref() == b"D:href" {
                        in_href = false;
                        if !current_href.is_empty() && !current_href.ends_with(base_path) {
                            // 提取文件名
                            if let Some(filename) = current_href.split('/').last() {
                                if !filename.is_empty() {
                                    files.push(filename.to_string());
                                }
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("解析 XML 失败: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(files)
    }

    /// 解析最后修改时间
    fn parse_last_modified(&self, xml: &str) -> Result<Option<i64>, String> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut in_lastmodified = false;
        let mut last_modified_str = String::new();

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = e.name();
                    if name.as_ref() == b"d:getlastmodified"
                        || name.as_ref() == b"D:getlastmodified"
                    {
                        in_lastmodified = true;
                        last_modified_str.clear();
                    }
                }
                Ok(Event::Text(e)) => {
                    if in_lastmodified {
                        last_modified_str.push_str(&String::from_utf8_lossy(&e));
                    }
                }
                Ok(Event::End(e)) => {
                    let name = e.name();
                    if name.as_ref() == b"d:getlastmodified"
                        || name.as_ref() == b"D:getlastmodified"
                    {
                        in_lastmodified = false;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("解析 XML 失败: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        if last_modified_str.is_empty() {
            return Ok(None);
        }

        // 解析 HTTP 日期格式
        use chrono::DateTime;
        if let Ok(dt) = DateTime::parse_from_rfc2822(&last_modified_str) {
            Ok(Some(dt.timestamp()))
        } else {
            Ok(None)
        }
    }
}
