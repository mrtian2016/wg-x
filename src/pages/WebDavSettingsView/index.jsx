import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useToast } from '../../hooks/useToast';
import Toast from '../../components/Toast';
import './style.css';

function WebDavSettingsView({ onBack, onConfigChange }) {
  const { messages, showToast, removeToast } = useToast();
  const [config, setConfig] = useState({
    enabled: false,
    server_url: '',
    username: '',
    password: '',
    sync_interval: 300,
    auto_sync_enabled: false,
  });

  const [loading, setLoading] = useState(true);
  const [testing, setTesting] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [testResult, setTestResult] = useState(null);
  const [syncResult, setSyncResult] = useState(null);
  const [lastSyncInfo, setLastSyncInfo] = useState(null); // 最后同步信息
  const [currentTime, setCurrentTime] = useState(Date.now()); // 用于触发时间更新

  // 加载配置和同步信息
  useEffect(() => {
    loadConfig();
    loadLastSyncInfo();
  }, []);

  

  // 定期从后端加载最新的同步信息（检测自动同步）
  useEffect(() => {
    const syncInfoTimer = setInterval(() => {
      loadLastSyncInfo();
      setCurrentTime(Date.now());
      
    }, 5000); // 每5秒检查一次

    return () => clearInterval(syncInfoTimer);
  }, []);

  // 注意：自动同步定时器已在 App.jsx 中全局管理，这里不再重复设置

  const loadConfig = async () => {
    try {
      const loadedConfig = await invoke('load_webdav_config');
      setConfig(loadedConfig);
    } catch (error) {
      console.error('加载配置失败:', error);
    } finally {
      setLoading(false);
    }
  };

  const loadLastSyncInfo = async () => {
    try {
      const syncInfo = await invoke('load_last_sync_info');
      setLastSyncInfo(syncInfo);
    } catch (error) {
      console.error('加载同步信息失败:', error);
    }
  };

  const handleSave = async () => {
    try {
      await invoke('save_webdav_config', { config });
      showToast('配置保存成功！', 'success');
      setTestResult(null); // 清除测试结果
      // 通知父组件配置已更改
      if (onConfigChange) {
        onConfigChange();
      }
    } catch (error) {
      showToast(`保存失败: ${error}`, 'error');
    }
  };

  const handleTest = async () => {
    if (!config.server_url || !config.username || !config.password) {
      showToast('请填写完整的服务器地址、用户名和密码', 'warning');
      return;
    }

    setTesting(true);
    setTestResult(null);

    try {
      await invoke('test_webdav_connection', { config });
      setTestResult({ success: true, message: '连接成功！' });
    } catch (error) {
      setTestResult({ success: false, message: `连接失败: ${error}` });
    } finally {
      setTesting(false);
    }
  };

  const handleSync = async () => {
    if (!config.enabled) {
      showToast('请先启用 WebDAV 同步并保存配置', 'warning');
      return;
    }

    setSyncing(true);
    setSyncResult(null);

    try {
      // 使用双向同步
      const result = await invoke('sync_bidirectional_webdav');

      setSyncResult({
        success: true,
        type: 'bidirectional',
        data: result,
      });

      // 重新加载同步信息
      await loadLastSyncInfo();
    } catch (error) {
      setSyncResult({
        success: false,
        message: `同步失败: ${error}`,
      });
    } finally {
      setSyncing(false);
    }
  };

  // 处理自动同步开关变化
  const handleAutoSyncToggle = async (enabled) => {
    const newConfig = { ...config, auto_sync_enabled: enabled };
    setConfig(newConfig);

    // 立即保存配置
    try {
      await invoke('save_webdav_config', { config: newConfig });
      // 通知父组件配置已更改
      if (onConfigChange) {
        onConfigChange();
      }
    } catch (error) {
      console.error('保存自动同步设置失败:', error);
      // 恢复原状态
      setConfig(config);
    }
  };

  const getSyncTypeText = (type) => {
    switch (type) {
      case 'bidirectional':
      case 'auto':
        return '双向同步';
      case 'upload':
        return '上传';
      case 'download':
        return '下载';
      default:
        return '同步';
    }
  };

  const formatLastSyncTime = () => {
    if (!lastSyncInfo || !lastSyncInfo.timestamp) return '从未同步';
    const lastSyncTimestamp = lastSyncInfo.timestamp * 1000; // 转换为毫秒
    const diff = Math.floor((currentTime - lastSyncTimestamp) / 1000); // 秒

    if (diff < 0) return '刚刚';
    if (diff < 60) return `${diff} 秒前`;
    if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    return `${Math.floor(diff / 86400)} 天前`;
  };

  if (loading) {
    return (
      <div className="form-section">
        <div className="webdav-settings-view">
          <div className="webdav-loading">加载配置中...</div>
        </div>
      </div>
    );
  }

  return (
    <div className="form-section">
      {/* Toast 消息通知 */}
      <Toast messages={messages} onRemove={removeToast} />

      <div className="webdav-settings-view">
        <div className="webdav-header">
          <h2>☁️ WebDAV 同步设置</h2>
          <button className="webdav-back-button" onClick={onBack}>
            ← 返回
          </button>
        </div>

        <div className="webdav-content">
          {/* 基本配置 */}
          <div className="webdav-section">
            <h3>基本设置</h3>
            <div className="webdav-form-group">
              <label className="webdav-checkbox-label">
                <input
                  type="checkbox"
                  checked={config.enabled}
                  onChange={(e) => setConfig({ ...config, enabled: e.target.checked })}
                />
                <span>启用 WebDAV 同步</span>
              </label>
            </div>

            <div className="webdav-form-group">
              <label>服务器地址</label>
              <input
                type="text"
                placeholder="https://your-webdav-server.com/dav"
                value={config.server_url}
                onChange={(e) => setConfig({ ...config, server_url: e.target.value })}
                disabled={!config.enabled}
              />
              <small className="webdav-help-text">
                WebDAV 服务器地址，例如：https://dav.example.com/remote.php/dav/files/username/
              </small>
            </div>

            <div className="webdav-form-group">
              <label>用户名</label>
              <input
                type="text"
                placeholder="用户名"
                value={config.username}
                onChange={(e) => setConfig({ ...config, username: e.target.value })}
                disabled={!config.enabled}
              />
            </div>

            <div className="webdav-form-group">
              <label>密码</label>
              <input
                type="password"
                placeholder="密码"
                value={config.password}
                onChange={(e) => setConfig({ ...config, password: e.target.value })}
                disabled={!config.enabled}
              />
              <small className="webdav-help-text webdav-warning">
                ⚠️ 密码将以明文存储在本地，请确保使用 HTTPS 连接
              </small>
            </div>

            <div className="webdav-form-group">
              <label>自动同步间隔（秒）</label>
              <input
                type="number"
                min="60"
                step="60"
                value={config.sync_interval}
                onChange={(e) => setConfig({ ...config, sync_interval: parseInt(e.target.value) || 300 })}
                disabled={!config.enabled}
              />
              <small className="webdav-help-text">
                设置自动同步的时间间隔，最少 60 秒。默认 300 秒（5 分钟）
              </small>
            </div>

            <div className="webdav-button-group">
              <button
                className="webdav-btn-primary"
                onClick={handleSave}
                disabled={!config.enabled || !config.server_url || !config.username || !config.password}
              >
                保存配置
              </button>
              <button
                className="webdav-btn-secondary"
                onClick={handleTest}
                disabled={testing || !config.server_url || !config.username || !config.password}
              >
                {testing ? '测试中...' : '测试连接'}
              </button>
            </div>

            {/* 测试结果 */}
            {testResult && (
              <div className={`webdav-test-result ${testResult.success ? 'webdav-success' : 'webdav-error'}`}>
                {testResult.success ? '✓' : '✗'} {testResult.message}
              </div>
            )}
          </div>

          {/* 同步控制 */}
          <div className="webdav-section">
            <h3>同步控制</h3>

            <div className="webdav-form-group">
              <div className="webdav-toggle-container">
                <label className="webdav-toggle-switch">
                  <input
                    type="checkbox"
                    checked={config.auto_sync_enabled}
                    onChange={(e) => handleAutoSyncToggle(e.target.checked)}
                    disabled={!config.enabled}
                  />
                  <span className="webdav-toggle-slider"></span>
                </label>
                <span className="webdav-toggle-label">
                  启用自动同步
                  {config.auto_sync_enabled && (
                    <span className="webdav-toggle-status">已启用</span>
                  )}
                </span>
              </div>
              {config.auto_sync_enabled && (
                <small className="webdav-help-text webdav-success" style={{ marginTop: '0.5rem' }}>
                  ✓ 自动同步已启用，将每 {config.sync_interval} 秒同步一次
                </small>
              )}
            </div>

            <div className="webdav-sync-status">
              <div>
                <span className="webdav-status-label">上次同步：</span>
                <span className="webdav-status-value">{formatLastSyncTime()}</span>
              </div>
              {lastSyncInfo && lastSyncInfo.sync_type && (
                <div >
                  <span className="webdav-status-label">同步模式：</span>
                  <span className="webdav-status-value webdav-sync-mode-badge">
                    {getSyncTypeText(lastSyncInfo.sync_type)}
                  </span>
                </div>
              )}
            </div>

            <div className="webdav-button-group">
              <button
                className="webdav-btn-sync"
                onClick={handleSync}
                disabled={syncing || !config.enabled}
              >
                {syncing ? '同步中...' : '立即同步'}
              </button>
            </div>

            {/* 同步结果 */}
            {syncResult && (
              <div className={`webdav-sync-result ${syncResult.success ? 'webdav-success' : 'webdav-error'}`}>
                {syncResult.success ? (
                  <>
                    <h4>✓ {getSyncTypeText(syncResult.type)}完成</h4>
                    <div className="webdav-sync-details">
                      <div className="webdav-sync-item">
                        <span className="webdav-label">服务端配置：</span>
                        <span className="webdav-value">
                          ↑ {syncResult.data.servers_uploaded} 个上传，
                          ↓ {syncResult.data.servers_downloaded} 个下载
                        </span>
                      </div>
                      <div className="webdav-sync-item">
                        <span className="webdav-label">历史记录：</span>
                        <span className="webdav-value">
                          ↑ {syncResult.data.history_uploaded} 个上传，
                          ↓ {syncResult.data.history_downloaded} 个下载
                        </span>
                      </div>
                    </div>
                  </>
                ) : (
                  <p>✗ {syncResult.message}</p>
                )}
              </div>
            )}
          </div>

          {/* 说明信息 */}
          <div className="webdav-section webdav-info-section">
            <h3>使用说明</h3>
            <ul className="webdav-info-list">
              <li>
                <strong>双向智能同步：</strong>自动比较本地和远程文件的时间戳，上传较新的本地文件，下载较新的远程文件
              </li>
              <li>
                <strong>仅上传：</strong>将所有本地配置和历史记录上传到云端（不会下载）
              </li>
              <li>
                <strong>仅下载：</strong>从云端下载所有配置和历史记录到本地（不会上传）
              </li>
              <li>
                <strong>自动同步：</strong>启用后，将按设定的时间间隔自动执行双向智能同步
              </li>
              <li>
                <strong>安全提示：</strong>请使用 HTTPS 协议连接 WebDAV 服务器，密码存储在本地配置文件中
              </li>
            </ul>
          </div>

          {/* 兼容的 WebDAV 服务 */}
          <div className="webdav-section webdav-info-section">
            <h3>兼容的 WebDAV 服务</h3>
            <ul className="webdav-info-list">
              <li>Nextcloud / ownCloud</li>
              <li>坚果云</li>
              <li>群晖 NAS (Synology)</li>
              <li>威联通 NAS (QNAP)</li>
              <li>阿里云盘 (WebDAV 第三方工具)</li>
              <li>其他标准 WebDAV 服务</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}

export default WebDavSettingsView;
