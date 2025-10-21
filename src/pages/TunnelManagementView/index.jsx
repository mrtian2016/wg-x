import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import ConfirmDialog from '../../components/ConfirmDialog';
import './style.css';

function TunnelManagementView({ onBack, onShowToast }) {
  const [tunnels, setTunnels] = useState([]);
  const [loading, setLoading] = useState(false);
  const [selectedTunnel, setSelectedTunnel] = useState(null);
  const [showConfigForm, setShowConfigForm] = useState(false);
  const [editingConfig, setEditingConfig] = useState(null);
  const [localPublicKey, setLocalPublicKey] = useState(''); // 本地公钥
  const [isLinux, setIsLinux] = useState(false); // 是否为 Linux 系统

  // 守护进程管理状态 (仅 Linux)
  const [daemonStatus, setDaemonStatus] = useState(null);
  const [daemonLogs, setDaemonLogs] = useState('');
  const [showDaemonPanel, setShowDaemonPanel] = useState(false);

  // 确认对话框状态
  const [confirmDialog, setConfirmDialog] = useState({
    isOpen: false,
    title: '',
    message: '',
    onConfirm: () => {},
  });

  // 配置表单状态
  const [config, setConfig] = useState({
    name: '',
    // Interface 配置
    privateKey: '',
    address: '',
    listenPort: '',
    dns: '',
    mtu: '1420',
    // Peer 配置 - 支持多个 Peer
    peers: [],
  });

  // 检测操作系统
  useEffect(() => {
    const checkPlatform = async () => {
      const platformName = await invoke('get_platform');
      console.log('当前操作系统:', platformName);
      setIsLinux(platformName === 'linux');
    };
    checkPlatform();
  }, []);

  // 加载守护进程状态 (仅 Linux)
  const loadDaemonStatus = async () => {
    if (!isLinux) return;

    try {
      const status = await invoke('check_daemon_status');
      setDaemonStatus(status);
    } catch (error) {
      console.error('获取守护进程状态失败:', error);
    }
  };

  // 加载隧道列表
  useEffect(() => {
    loadTunnels();
    loadDaemonStatus(); // 同时加载守护进程状态

    // 每 2 秒刷新一次隧道状态
    // 但在配置表单打开时暂停轮询,避免 Linux 上的 UI 卡顿
    const interval = setInterval(() => {
      if (!showConfigForm) {
        loadTunnels();
        loadDaemonStatus();
      }
    }, 2000);
    return () => clearInterval(interval);
  }, [showConfigForm, isLinux]);

  const loadTunnels = async () => {
    // 防止重复请求(如果正在加载或表单打开,则跳过)
    if (loading || showConfigForm) {
      return;
    }

    setLoading(true);
    try {
      const list = await invoke('get_all_tunnel_configs');
      setTunnels(list);
    } catch (error) {
      console.error('加载隧道列表失败:', error);
    } finally {
      setLoading(false);
    }
  };

  // 生成密钥对
  const handleGenerateKeyPair = async () => {
    try {
      const keypair = await invoke('generate_keypair');
      setConfig({ ...config, privateKey: keypair.private_key });
      setLocalPublicKey(keypair.public_key);
      onShowToast('密钥对已生成,公钥已显示', 'success');
    } catch (error) {
      onShowToast('生成密钥失败: ' + error, 'error');
    }
  };

  // 从私钥计算公钥
  const handleCalculatePublicKey = async () => {
    if (!config.privateKey || config.privateKey.trim() === '') {
      // 私钥为空时静默返回,不显示提示
      return;
    }
    try {
      const publicKey = await invoke('private_key_to_public', { privateKey: config.privateKey });
      setLocalPublicKey(publicKey);
      onShowToast('公钥已计算', 'success');
    } catch (error) {
      onShowToast('计算公钥失败: ' + error, 'error');
    }
  };

  // 复制公钥到剪贴板
  const handleCopyPublicKey = async () => {
    try {
      await navigator.clipboard.writeText(localPublicKey);
      onShowToast('公钥已复制到剪贴板', 'success');
    } catch (error) {
      onShowToast('复制失败: ' + error, 'error');
    }
  };

  // 生成预共享密钥
  const handleGeneratePresharedKey = async (peerIndex) => {
    try {
      const psk = await invoke('generate_preshared_key');
      const newPeers = [...config.peers];
      newPeers[peerIndex].presharedKey = psk;
      setConfig({ ...config, peers: newPeers });
      onShowToast('预共享密钥已生成', 'success');
    } catch (error) {
      onShowToast('生成预共享密钥失败: ' + error, 'error');
    }
  };

  // 添加 Peer
  const handleAddPeer = () => {
    setConfig({
      ...config,
      peers: [
        ...config.peers,
        {
          publicKey: '',
          presharedKey: '',
          endpoint: '',
          allowedIps: '0.0.0.0/0',
          persistentKeepalive: 25,
        },
      ],
    });
  };

  // 删除 Peer
  const handleRemovePeer = (index) => {
    const newPeers = config.peers.filter((_, i) => i !== index);
    setConfig({ ...config, peers: newPeers });
  };

  // 更新 Peer
  const handleUpdatePeer = (index, field, value) => {
    const newPeers = [...config.peers];
    newPeers[index][field] = value;
    setConfig({ ...config, peers: newPeers });
  };

  // 保存隧道配置
  const handleSaveConfig = async () => {
    try {
      // 验证必填字段
      if (!config.name) {
        onShowToast('请输入隧道名称', 'warning');
        return;
      }
      if (!config.privateKey) {
        onShowToast('请生成或输入私钥', 'warning');
        return;
      }
      if (!config.address) {
        onShowToast('请输入本地 IP 地址', 'warning');
        return;
      }

      // 验证 Peer 配置
      for (let i = 0; i < config.peers.length; i++) {
        const peer = config.peers[i];
        if (!peer.publicKey) {
          onShowToast(`Peer ${i + 1}: 请输入对端公钥`, 'warning');
          return;
        }
        if (!peer.allowedIps) {
          onShowToast(`Peer ${i + 1}: 请输入 AllowedIPs`, 'warning');
          return;
        }
      }

      setLoading(true);

      // 构建要保存的配置对象
      const tunnelConfig = {
        id: editingConfig ? editingConfig.id : Date.now().toString(),
        name: config.name,
        private_key: config.privateKey,
        address: config.address,
        listen_port: String(config.listenPort || ''), // 确保是字符串
        dns: config.dns || '',
        mtu: String(config.mtu || '1420'), // 确保是字符串
        peers: config.peers.map(peer => ({
          public_key: peer.publicKey,
          preshared_key: peer.presharedKey || null,
          endpoint: peer.endpoint || null,
          allowed_ips: peer.allowedIps,
          persistent_keepalive: peer.persistentKeepalive || null,
        })),
        // 保留旧格式以向后兼容
        peer_public_key: '',
        preshared_key: '',
        endpoint: '',
        allowed_ips: '',
        persistent_keepalive: '',
        created_at:  Date.now(),
      };

      await invoke('save_tunnel_config', { config: tunnelConfig });
      onShowToast('隧道配置已保存', 'success');
      setShowConfigForm(false);
      resetForm();
      await loadTunnels();
    } catch (error) {
      onShowToast('保存配置失败: ' + error, 'error');
    } finally {
      setLoading(false);
    }
  };

  // 重置表单
  const resetForm = () => {
    setConfig({
      name: '',
      privateKey: '',
      address: '',
      listenPort: '',
      dns: '',
      mtu: '1420',
      peers: [],
    });
    setLocalPublicKey('');
    setEditingConfig(null);
  };

  // 编辑隧道配置
  const handleEditTunnel = async (tunnel) => {
    try {
      // 从后端获取完整配置(包括私钥、peers等)
      const fullConfig = await invoke('get_tunnel_config', { tunnelId: tunnel.id });

      // 转换为表单格式
      const peers = fullConfig.peers && fullConfig.peers.length > 0
        ? fullConfig.peers.map(p => ({
            publicKey: p.public_key || '',
            presharedKey: p.preshared_key || '',
            endpoint: p.endpoint || '',
            allowedIps: p.allowed_ips || '0.0.0.0/0',
            persistentKeepalive: p.persistent_keepalive || 25,
          }))
        : [];

      // 如果没有 peers 数组但有旧格式的单个 peer
      if (peers.length === 0 && fullConfig.peer_public_key) {
        peers.push({
          publicKey: fullConfig.peer_public_key || '',
          presharedKey: fullConfig.preshared_key || '',
          endpoint: fullConfig.endpoint || '',
          allowedIps: fullConfig.allowed_ips || '0.0.0.0/0',
          persistentKeepalive: parseInt(fullConfig.persistent_keepalive) || 25,
        });
      }

      setConfig({
        name: fullConfig.name,
        privateKey: fullConfig.private_key || '',
        address: fullConfig.address || '',
        listenPort: fullConfig.listen_port || '',
        dns: fullConfig.dns || '',
        mtu: fullConfig.mtu || '1420',
        peers,
      });

      // 如果有私钥,计算公钥
      if (fullConfig.private_key) {
        try {
          const publicKey = await invoke('private_key_to_public', { privateKey: fullConfig.private_key });
          setLocalPublicKey(publicKey);
        } catch (error) {
          console.error('计算公钥失败:', error);
        }
      }

      setEditingConfig(fullConfig);
      setShowConfigForm(true);
    } catch (error) {
      onShowToast('加载配置失败: ' + error, 'error');
    }
  };

  // 启动隧道
  const handleStartTunnel = async (tunnelId) => {
    try {
      console.log('启动隧道:', tunnelId);
      setLoading(true);
      await invoke('start_tunnel', { tunnelId });
      onShowToast('隧道启动成功', 'success');
      await loadTunnels();
    } catch (error) {
      console.error('启动隧道失败:', error);
      onShowToast('启动隧道失败: ' + error, 'error');
    } finally {
      console.log('重置 loading 状态');
      setLoading(false);
    }
  };

  // 停止隧道
  const handleStopTunnel = async (tunnelId) => {
    try {
      setLoading(true);
      await invoke('stop_tunnel', { tunnelId });
      onShowToast('隧道已停止', 'success');
      await loadTunnels();
    } catch (error) {
      onShowToast('停止隧道失败: ' + error, 'error');
    } finally {
      setLoading(false);
    }
  };

  // 删除隧道配置
  const handleDeleteTunnel = (tunnelId) => {
    setConfirmDialog({
      isOpen: true,
      title: '删除隧道',
      message: '确定要删除此隧道配置吗?',
      onConfirm: async () => {
        setConfirmDialog({ ...confirmDialog, isOpen: false });
        try {
          setLoading(true);
          await invoke('delete_tunnel_config', { tunnelId });
          onShowToast('隧道配置已删除', 'success');
          await loadTunnels();
        } catch (error) {
          onShowToast('删除配置失败: ' + error, 'error');
        } finally {
          setLoading(false);
        }
      },
    });
  };

  // 查看隧道详情
  const handleViewDetails = async (tunnelId) => {
    try {
      const details = await invoke('get_tunnel_details', { tunnelId });
      setSelectedTunnel(details);
    } catch (error) {
      onShowToast('获取隧道详情失败: ' + error, 'error');
    }
  };

  // 格式化流量
  const formatBytes = (bytes) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return (bytes / Math.pow(k, i)).toFixed(2) + ' ' + sizes[i];
  };

  // 格式化时间
  const formatTime = (timestamp) => {
    if (!timestamp) return '从未';
    const date = new Date(timestamp * 1000);
    const now = new Date();
    const diff = Math.floor((now - date) / 1000);

    // 2分钟内显示秒数,更精确
    if (diff < 120) return `${diff} 秒前`;
    // 1小时内显示分钟数
    if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
    // 1天内显示小时数
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    // 超过1天显示天数
    return `${Math.floor(diff / 86400)} 天前`;
  };

  // ========== 守护进程管理函数 (仅 Linux) ==========

  // 安装守护进程
  const handleInstallDaemon = () => {
    setConfirmDialog({
      isOpen: true,
      title: '安装守护进程',
      message: '确定要安装守护进程吗? 这需要管理员权限。',
      onConfirm: async () => {
        setConfirmDialog({ ...confirmDialog, isOpen: false });
        setLoading(true);
        try {
          const result = await invoke('install_daemon');
          onShowToast(`安装成功!\n\n${result}`, 'success');
          await loadDaemonStatus();
        } catch (error) {
          const errorMsg = String(error);
          if (errorMsg.includes('取消')) {
            onShowToast('用户取消了授权', 'warning');
          } else {
            onShowToast(`安装失败: ${errorMsg}`, 'error');
          }
        } finally {
          setLoading(false);
        }
      },
    });
  };

  // 卸载守护进程
  const handleUninstallDaemon = () => {
    setConfirmDialog({
      isOpen: true,
      title: '卸载守护进程',
      message: '确定要卸载守护进程吗? 这将停止所有隧道并删除守护进程。',
      onConfirm: async () => {
        setConfirmDialog({ ...confirmDialog, isOpen: false });
        setLoading(true);
        try {
          const result = await invoke('uninstall_daemon');
          onShowToast(`卸载成功!\n\n${result}`, 'success');
          await loadDaemonStatus();
        } catch (error) {
          const errorMsg = String(error);
          if (errorMsg.includes('取消')) {
            onShowToast('用户取消了授权', 'warning');
          } else {
            onShowToast(`卸载失败: ${errorMsg}`, 'error');
          }
        } finally {
          setLoading(false);
        }
      },
    });
  };

  // 启动守护进程
  const handleStartDaemon = async () => {
    setLoading(true);
    try {
      await invoke('start_daemon_service');
      onShowToast('守护进程已启动', 'success');
      await loadDaemonStatus();
    } catch (error) {
      onShowToast(`启动失败: ${error}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  // 停止守护进程
  const handleStopDaemon = () => {
    setConfirmDialog({
      isOpen: true,
      title: '停止守护进程',
      message: '确定要停止守护进程吗? 这将停止所有运行中的隧道。',
      onConfirm: async () => {
        setConfirmDialog({ ...confirmDialog, isOpen: false });
        setLoading(true);
        try {
          await invoke('stop_daemon_service');
          onShowToast('守护进程已停止', 'success');
          await loadDaemonStatus();
        } catch (error) {
          onShowToast(`停止失败: ${error}`, 'error');
        } finally {
          setLoading(false);
        }
      },
    });
  };

  // 重启守护进程
  const handleRestartDaemon = async () => {
    setLoading(true);
    try {
      await invoke('restart_daemon_service');
      onShowToast('守护进程已重启', 'success');
      await loadDaemonStatus();
    } catch (error) {
      onShowToast(`重启失败: ${error}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  // 启用开机自启
  const handleEnableDaemon = async () => {
    setLoading(true);
    try {
      await invoke('enable_daemon_service');
      onShowToast('已启用开机自启', 'success');
      await loadDaemonStatus();
    } catch (error) {
      onShowToast(`启用失败: ${error}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  // 禁用开机自启
  const handleDisableDaemon = async () => {
    setLoading(true);
    try {
      await invoke('disable_daemon_service');
      onShowToast('已禁用开机自启', 'success');
      await loadDaemonStatus();
    } catch (error) {
      onShowToast(`禁用失败: ${error}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  // 获取守护进程日志
  const handleViewDaemonLogs = async () => {
    setLoading(true);
    try {
      const result = await invoke('get_daemon_logs', { lines: 100 });
      setDaemonLogs(result);
    } catch (error) {
      onShowToast(`获取日志失败: ${error}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="tunnel-management-view">

      <div className="tunnel-actions">
        <button
          onClick={() => {
            resetForm();
            setShowConfigForm(true);
          }}
          className="btn-primary"
          disabled={loading}
        >
          + 新建隧道
        </button>
        <button
          onClick={loadTunnels}
          className="btn-secondary"
          disabled={loading}
        >
          🔄 刷新
        </button>
        {/* Linux 守护进程管理按钮 */}
        {isLinux && daemonStatus && (
          <button
            onClick={() => setShowDaemonPanel(!showDaemonPanel)}
            className={daemonStatus.running ? "btn-success" : "btn-warning"}
            title={daemonStatus.running ? "守护进程运行中" : "守护进程未运行"}
          >
            ⚙️ 守护进程 {daemonStatus.running ? '🟢' : '🔴'}
          </button>
        )}
      </div>

      {/* Linux 守护进程管理面板 */}
      {isLinux && showDaemonPanel && daemonStatus && (
        <div className="daemon-panel" style={{
          background: '#f8f9fa',
          border: '1px solid #dee2e6',
          borderRadius: '8px',
          padding: '1.5rem',
          marginBottom: '1.5rem'
        }}>
          <div style={{display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem'}}>
            <h3 style={{margin: 0}}>Linux 守护进程管理</h3>
            <button onClick={() => setShowDaemonPanel(false)} className="btn-close" style={{background: 'transparent', border: 'none', fontSize: '1.5rem', cursor: 'pointer'}}>
              ✕
            </button>
          </div>

          {/* 状态信息 */}
          <div className="daemon-status" style={{
            background: 'white',
            padding: '1rem',
            borderRadius: '6px',
            marginBottom: '1rem',
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
            gap: '1rem'
          }}>
            <div>
              <strong>安装状态:</strong> {daemonStatus.installed ? '✓ 已安装' : '✗ 未安装'}
            </div>
            <div>
              <strong>运行状态:</strong> {daemonStatus.running ? '🟢 运行中' : '🔴 已停止'}
            </div>
            <div>
              <strong>开机自启:</strong> {daemonStatus.enabled ? '✓ 已启用' : '✗ 未启用'}
            </div>
            {daemonStatus.version && (
              <div>
                <strong>版本:</strong> {daemonStatus.version}
              </div>
            )}
          </div>

          {/* 操作按钮 */}
          <div className="daemon-actions">
            {!daemonStatus.installed ? (
              <button onClick={handleInstallDaemon} className="btn-primary" disabled={loading}>
                📦 安装守护进程
              </button>
            ) : (
              <>
                <div style={{display: 'flex', gap: '0.5rem', flexWrap: 'wrap', marginBottom: '0.5rem'}}>
                  <button onClick={handleStartDaemon} className="btn-success" disabled={loading || daemonStatus.running}>
                    ▶️ 启动
                  </button>
                  <button onClick={handleStopDaemon} className="btn-danger" disabled={loading || !daemonStatus.running}>
                    ⏹️ 停止
                  </button>
                  <button onClick={handleRestartDaemon} className="btn-secondary" disabled={loading}>
                    🔄 重启
                  </button>
                  <button onClick={handleViewDaemonLogs} className="btn-secondary" disabled={loading}>
                    📋 查看日志
                  </button>
                </div>
                <div style={{display: 'flex', gap: '0.5rem', flexWrap: 'wrap'}}>
                  <button onClick={handleEnableDaemon} className="btn-secondary" disabled={loading || daemonStatus.enabled}>
                    ✓ 启用开机自启
                  </button>
                  <button onClick={handleDisableDaemon} className="btn-secondary" disabled={loading || !daemonStatus.enabled}>
                    ✗ 禁用开机自启
                  </button>
                  <button onClick={handleUninstallDaemon} className="btn-danger-outline" disabled={loading} style={{marginLeft: 'auto'}}>
                    🗑️ 卸载守护进程
                  </button>
                </div>
              </>
            )}
          </div>

          {/* 日志显示 */}
          {daemonLogs && (
            <div className="daemon-logs" style={{marginTop: '1rem'}}>
              <div style={{display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.5rem'}}>
                <h4 style={{margin: 0}}>日志 (最近 100 行)</h4>
                <button onClick={() => setDaemonLogs('')} className="btn-secondary" style={{fontSize: '0.875rem', padding: '0.25rem 0.5rem'}}>
                  关闭日志
                </button>
              </div>
              <pre style={{
                background: '#000',
                color: '#0f0',
                padding: '1rem',
                overflow: 'auto',
                maxHeight: '400px',
                fontSize: '12px',
                fontFamily: 'monospace',
                borderRadius: '4px',
                margin: 0
              }}>
                {daemonLogs}
              </pre>
            </div>
          )}
        </div>
      )}

      {tunnels.length === 0 ? (
        <div className="empty-state">
          <div className="empty-icon">🚇</div>
          <h3>暂无隧道配置</h3>
          <p>点击"新建隧道"按钮创建你的第一个 WireGuard 隧道</p>
          <button
            onClick={() => {
              resetForm();
              setShowConfigForm(true);
            }}
            className="btn-primary"
          >
            + 新建隧道
          </button>
        </div>
      ) : (
        <div className="tunnel-list">
          {tunnels.map((tunnel) => (
            <div key={tunnel.id} className="tunnel-card">
              <div className="tunnel-card-header">
                <div className="tunnel-info">
                  <h3>{tunnel.name}</h3>
                  <span className={`tunnel-status status-${tunnel.status}`}>
                    {tunnel.status === 'running' ? '🟢 运行中' :
                     tunnel.status === 'stopped' ? '🔴 已停止' :
                     '🟡 连接中'}
                  </span>
                </div>
                <div className="tunnel-actions-inline">
                  {tunnel.status === 'running' ? (
                    <button
                      onClick={() => handleStopTunnel(tunnel.id)}
                      className="btn-danger"
                      disabled={loading}
                      title={loading ? '操作中...' : '停止隧道'}
                    >
                      停止
                    </button>
                  ) : (
                    <button
                      onClick={() => handleStartTunnel(tunnel.id)}
                      className="btn-success"
                      disabled={loading}
                      title={loading ? '操作中...' : '启动隧道'}
                    >
                      启动
                    </button>
                  )}
                  <button
                    onClick={() => handleEditTunnel(tunnel)}
                    className="btn-secondary"
                    disabled={loading || tunnel.status === 'running'}
                    title={tunnel.status === 'running' ? '请先停止隧道' : '编辑配置'}
                  >
                    编辑
                  </button>
                  <button
                    onClick={() => handleViewDetails(tunnel.id)}
                    className="btn-secondary"
                  >
                    详情
                  </button>
                  <button
                    onClick={() => handleDeleteTunnel(tunnel.id)}
                    className="btn-danger-outline"
                    disabled={loading || tunnel.status === 'running'}
                    title={tunnel.status === 'running' ? '请先停止隧道' : '删除配置'}
                  >
                    删除
                  </button>
                </div>
              </div>

              <div className="tunnel-card-body">
                <div className="tunnel-stat">
                  <span className="stat-label">本地地址:</span>
                  <span className="stat-value">{tunnel.address || 'N/A'}</span>
                </div>
                <div className="tunnel-stat">
                  <span className="stat-label">对端:</span>
                  <span className="stat-value">{tunnel.endpoint || 'N/A'}</span>
                </div>
                {tunnel.status === 'running' && (
                  <>
                    <div className="tunnel-stat">
                      <span className="stat-label">上传:</span>
                      <span className="stat-value">{formatBytes(tunnel.tx_bytes || 0)}</span>
                    </div>
                    <div className="tunnel-stat">
                      <span className="stat-label">下载:</span>
                      <span className="stat-value">{formatBytes(tunnel.rx_bytes || 0)}</span>
                    </div>
                    <div className="tunnel-stat">
                      <span className="stat-label">最后握手:</span>
                      <span className="stat-value">{formatTime(tunnel.last_handshake)}</span>
                    </div>
                  </>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* 隧道配置表单模态框 */}
      {showConfigForm && (
        <div className="modal-overlay" onClick={() => setShowConfigForm(false)}>
          <div className="modal-content config-modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3>{editingConfig ? '编辑隧道配置' : '新建 WireGuard 隧道'}</h3>
              <button
                onClick={() => setShowConfigForm(false)}
                className="btn-close"
              >
                ✕
              </button>
            </div>
            <div className="modal-body">
              {/* 基本信息 */}
              <div className="config-section">
                <h4>基本信息</h4>
                <div className="form-group">
                  <label>隧道名称 *</label>
                  <input
                    type="text"
                    value={config.name}
                    onChange={(e) => setConfig({ ...config, name: e.target.value })}
                    placeholder="例如: 我的 VPN"
                  />
                </div>
              </div>

              {/* Interface 配置 */}
              <div className="config-section">
                <h4>Interface (本地接口)</h4>

                <div className="form-group">
                  <label>私钥 *</label>
                  <div className="input-with-button">
                    <input
                      type="text"
                      value={config.privateKey}
                      onChange={(e) => {
                        setConfig({ ...config, privateKey: e.target.value });
                        setLocalPublicKey(''); // 清空公钥,等待重新计算
                      }}
                      onBlur={handleCalculatePublicKey}
                      placeholder="点击生成或手动输入私钥"
                      className="monospace-input"
                    />
                    <button onClick={handleGenerateKeyPair} className="btn-inline">
                      生成密钥
                    </button>
                  </div>
                </div>

                {/* 显示公钥 */}
                {localPublicKey && (
                  <div className="form-group" style={{background: '#f0f8ff', padding: '1rem', borderRadius: '8px', border: '1px solid #b3d9ff'}}>
                    <label style={{display: 'flex', justifyContent: 'space-between', alignItems: 'center'}}>
                      <span>📢 本地公钥 (提供给对端)</span>
                      <button
                        onClick={handleCopyPublicKey}
                        className="btn-inline"
                        type="button"
                        style={{fontSize: '0.875rem', padding: '0.25rem 0.5rem'}}
                      >
                        📋 复制
                      </button>
                    </label>
                    <div style={{
                      marginTop: '0.5rem',
                      padding: '0.75rem',
                      background: 'white',
                      borderRadius: '4px',
                      fontFamily: 'monospace',
                      fontSize: '0.875rem',
                      wordBreak: 'break-all',
                      border: '1px solid #dee2e6'
                    }}>
                      {localPublicKey}
                    </div>
                    <small style={{display: 'block', marginTop: '0.5rem', color: '#0066cc'}}>
                      ℹ️ 对端配置 Peer 时需要使用这个公钥
                    </small>
                  </div>
                )}

                <div className="form-group">
                  <label>本地 IP 地址 *</label>
                  <input
                    type="text"
                    value={config.address}
                    onChange={(e) => setConfig({ ...config, address: e.target.value })}
                    placeholder="例如: 10.0.0.2/24"
                  />
                  <small>格式: IP/子网掩码位数</small>
                </div>

                <div className="form-row">
                  <div className="form-group">
                    <label>监听端口</label>
                    <input
                      type="number"
                      value={config.listenPort}
                      onChange={(e) => setConfig({ ...config, listenPort: e.target.value })}
                      placeholder="留空自动分配"
                    />
                  </div>
                  <div className="form-group">
                    <label>MTU</label>
                    <input
                      type="number"
                      value={config.mtu}
                      onChange={(e) => setConfig({ ...config, mtu: e.target.value })}
                      placeholder="1420"
                    />
                  </div>
                </div>

                <div className="form-group">
                  <label>DNS 服务器</label>
                  <input
                    type="text"
                    value={config.dns}
                    onChange={(e) => setConfig({ ...config, dns: e.target.value })}
                    placeholder="例如: 1.1.1.1, 8.8.8.8"
                  />
                  <small>多个 DNS 用逗号分隔</small>
                </div>
              </div>

              {/* Peer 配置 - 支持多个 */}
              <div className="config-section">
                <div style={{display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem'}}>
                  <div>
                    <h4 style={{margin: 0}}>Peer (对端配置)</h4>
                    <small style={{color: '#6c757d'}}>
                      如果作为服务端运行，可以不添加 Peer，等待客户端连接
                    </small>
                  </div>
                  <button
                    onClick={handleAddPeer}
                    className="btn-inline"
                    type="button"
                  >
                    + 添加 Peer
                  </button>
                </div>

                {config.peers.length === 0 ? (
                  <div style={{padding: '2rem', textAlign: 'center', background: '#f8f9fa', borderRadius: '8px'}}>
                    <p style={{margin: 0, color: '#6c757d'}}>暂无 Peer 配置</p>
                    <small style={{color: '#999'}}>点击"添加 Peer"按钮添加对端配置</small>
                  </div>
                ) : (
                  config.peers.map((peer, index) => (
                    <div key={index} className="peer-config-group" style={{
                      border: '1px solid #dee2e6',
                      borderRadius: '8px',
                      padding: '1rem',
                      marginBottom: '1rem',
                      background: '#f8f9fa'
                    }}>
                      <div style={{display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem'}}>
                        <h5 style={{margin: 0}}>Peer {index + 1}</h5>
                        <button
                          onClick={() => handleRemovePeer(index)}
                          className="btn-danger-outline"
                          type="button"
                          style={{padding: '0.25rem 0.5rem', fontSize: '0.875rem'}}
                        >
                          删除
                        </button>
                      </div>

                      <div className="form-group">
                        <label>对端公钥 *</label>
                        <input
                          type="text"
                          value={peer.publicKey}
                          onChange={(e) => handleUpdatePeer(index, 'publicKey', e.target.value)}
                          placeholder="输入对端的公钥"
                          className="monospace-input"
                        />
                      </div>

                      <div className="form-group">
                        <label>预共享密钥 (可选)</label>
                        <div className="input-with-button">
                          <input
                            type="text"
                            value={peer.presharedKey}
                            onChange={(e) => handleUpdatePeer(index, 'presharedKey', e.target.value)}
                            placeholder="点击生成或手动输入"
                            className="monospace-input"
                          />
                          <button
                            onClick={() => handleGeneratePresharedKey(index)}
                            className="btn-inline"
                            type="button"
                          >
                            生成 PSK
                          </button>
                        </div>
                      </div>

                      <div className="form-group">
                        <label>对端地址 (Endpoint)</label>
                        <input
                          type="text"
                          value={peer.endpoint}
                          onChange={(e) => handleUpdatePeer(index, 'endpoint', e.target.value)}
                          placeholder="例如: vpn.example.com:51820"
                        />
                        <small>格式: 域名或IP:端口</small>
                      </div>

                      <div className="form-group">
                        <label>允许的 IP (AllowedIPs) *</label>
                        <input
                          type="text"
                          value={peer.allowedIps}
                          onChange={(e) => handleUpdatePeer(index, 'allowedIps', e.target.value)}
                          placeholder="0.0.0.0/0"
                        />
                        <small>0.0.0.0/0 表示所有流量,多个IP用逗号分隔</small>
                      </div>

                      <div className="form-group">
                        <label>保持连接 (PersistentKeepalive)</label>
                        <input
                          type="number"
                          value={peer.persistentKeepalive}
                          onChange={(e) => handleUpdatePeer(index, 'persistentKeepalive', parseInt(e.target.value) || 0)}
                          placeholder="25"
                        />
                        <small>NAT 穿透保持连接间隔(秒), 0 表示禁用</small>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </div>
            <div className="modal-footer">
              <button
                onClick={() => setShowConfigForm(false)}
                className="btn-secondary"
              >
                取消
              </button>
              <button
                onClick={handleSaveConfig}
                className="btn-primary"
                disabled={loading}
              >
                {loading ? '保存中...' : '保存配置'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 隧道详情模态框 */}
      {selectedTunnel && (
        <div className="modal-overlay" onClick={() => setSelectedTunnel(null)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3>隧道详情</h3>
              <button
                onClick={() => setSelectedTunnel(null)}
                className="btn-close"
              >
                ✕
              </button>
            </div>
            <div className="modal-body">
              <div className="detail-group">
                <label>隧道名称:</label>
                <div>{selectedTunnel.name}</div>
              </div>
              <div className="detail-group">
                <label>状态:</label>
                <div>
                  <span className={`tunnel-status status-${selectedTunnel.status}`}>
                    {selectedTunnel.status === 'running' ? '🟢 运行中' : '🔴 已停止'}
                  </span>
                </div>
              </div>
              <div className="detail-group">
                <label>本地地址:</label>
                <div>{selectedTunnel.address}</div>
              </div>
              <div className="detail-group">
                <label>监听端口:</label>
                <div>{selectedTunnel.listen_port || 'Auto'}</div>
              </div>
              <div className="detail-group">
                <label>对端地址:</label>
                <div>{selectedTunnel.endpoint}</div>
              </div>
              <div className="detail-group">
                <label>AllowedIPs:</label>
                <div>{selectedTunnel.allowed_ips}</div>
              </div>
              <div className="detail-group">
                <label>公钥:</label>
                <div className="code-block">{selectedTunnel.public_key}</div>
              </div>
              {selectedTunnel.status === 'running' && (
                <>
                  <div className="detail-group">
                    <label>上传流量:</label>
                    <div>{formatBytes(selectedTunnel.tx_bytes || 0)}</div>
                  </div>
                  <div className="detail-group">
                    <label>下载流量:</label>
                    <div>{formatBytes(selectedTunnel.rx_bytes || 0)}</div>
                  </div>
                  <div className="detail-group">
                    <label>最后握手:</label>
                    <div>{formatTime(selectedTunnel.last_handshake)}</div>
                  </div>
                </>
              )}
            </div>
            <div className="modal-footer">
              <button
                onClick={() => setSelectedTunnel(null)}
                className="btn-primary"
              >
                关闭
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 确认对话框 */}
      <ConfirmDialog
        isOpen={confirmDialog.isOpen}
        title={confirmDialog.title}
        message={confirmDialog.message}
        onConfirm={confirmDialog.onConfirm}
        onCancel={() => setConfirmDialog({ ...confirmDialog, isOpen: false })}
      />
    </div>
  );
}

export default TunnelManagementView;
