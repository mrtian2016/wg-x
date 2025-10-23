import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import ConfirmDialog from '../../components/ConfirmDialog';
import DaemonPanel from '../../components/DaemonPanel';
import './style.css';

function TunnelManagementView({ onShowToast }) {
  const [tunnels, setTunnels] = useState([]);
  const [loading, setLoading] = useState(false);
  const [selectedTunnel, setSelectedTunnel] = useState(null);
  const [showConfigForm, setShowConfigForm] = useState(false);
  const [editingConfig, setEditingConfig] = useState(null);
  const [localPublicKey, setLocalPublicKey] = useState(''); // 本地公钥
  const [isLinux, setIsLinux] = useState(false); // 是否为 Linux 系统

  // 守护进程管理状态 (仅 Linux)
  const [daemonStatus, setDaemonStatus] = useState(null);
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
    mode: '', // 'server' 或 'client'
    // Interface 配置
    privateKey: '',
    address: '',
    listenPort: '',
    dns: '',
    mtu: '1420',
    serverEndpoint: '', // 服务端的公网 IP 或域名（仅服务端）
    // Peer 配置 - 支持多个 Peer (服务端) 或单个 Peer (客户端)
    peers: [],
  });

  // 模式选择对话框状态
  const [showModeSelector, setShowModeSelector] = useState(false);

  // Peer 列表显示状态
  const [activePeerTab, setActivePeerTab] = useState(0); // 当前选中的 Peer Tab 索引
  const [activePeerConfigTab, setActivePeerConfigTab] = useState('wireguard'); // 当前选中的 Peer 配置 Tab ('wireguard', 'qrcode', 'surge')


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

  // 快速添加客户端（自动生成密钥对和私钥）
  const handleQuickAddClient = async () => {
    try {
      // 为客户端生成密钥对
      const clientKeypair = await invoke('generate_keypair');

      // 生成预共享密钥
      const psk = await invoke('generate_preshared_key');

      // 自动生成客户端 IP（基于当前地址段）
      let clientIp = '10.0.0.2/32'; // 默认值
      if (config.address) {
        const parts = config.address.split('/');
        const baseIp = parts[0].split('.');
        const lastOctet = parseInt(baseIp[3]) + config.peers.length + 1;
        clientIp = `${baseIp[0]}.${baseIp[1]}.${baseIp[2]}.${lastOctet}/32`;
      }

      // 添加新 Peer（包含客户端的临时私钥）
      const newPeer = {
        publicKey: clientKeypair.public_key,
        clientPrivateKey: clientKeypair.private_key, // 保存客户端私钥，用于生成完整配置
        presharedKey: psk,
        endpoint: '', // 服务端模式下不需要 endpoint
        allowedIps: clientIp,
        persistentKeepalive: 0, // 服务端默认为 0，不需要保持连接
      };

      setConfig({
        ...config,
        peers: [...config.peers, newPeer],
      });

      onShowToast('客户端已添加，密钥对和预共享密钥已自动生成', 'success');
    } catch (error) {
      onShowToast('快速添加客户端失败: ' + error, 'error');
    }
  };

  // 生成客户端配置预览
  const generateClientConfigPreview = (peerIndex) => {
    const peer = config.peers[peerIndex];
    if (!config.privateKey || !config.address || !peer.publicKey) {
      return '请先完善服务端和客户端的基本配置';
    }

    // 获取服务端的 Endpoint
    let serverEndpoint = '服务端地址未配置';
    if (config.serverEndpoint) {
      serverEndpoint = config.listenPort ?
        `${config.serverEndpoint}:${config.listenPort}` :
        `${config.serverEndpoint}:51820`;
    } else {
      serverEndpoint = config.listenPort ?
        `<服务器IP或域名>:${config.listenPort}` :
        '<服务器IP或域名>:51820';
    }

    // 从 Interface 的 AllowedIPs 提取服务端的网络段（用于 Peer 中的 AllowedIPs）
    const serverAllowedIps = config.address || '10.0.0.0/24';

    // 使用保存的客户端私钥，如果没有则使用占位符
    const clientPrivateKey = peer.clientPrivateKey || '<客户端私钥>';

    // 生成客户端配置内容 - 完整可用的配置
    const clientConfig = `[Interface]
PrivateKey = ${clientPrivateKey}
Address = ${peer.allowedIps}
${config.dns ? `DNS = ${config.dns}` : 'DNS = 8.8.8.8, 8.8.4.4'}
${config.mtu ? `MTU = ${config.mtu}` : 'MTU = 1420'}

[Peer]
PublicKey = ${localPublicKey || '<服务端公钥>'}
${peer.presharedKey ? `PreSharedKey = ${peer.presharedKey}` : '# PreSharedKey = <预共享密钥，可选>'}
AllowedIPs = ${serverAllowedIps}
Endpoint = ${serverEndpoint}
PersistentKeepalive = 25`;

    return clientConfig;
  };

  // 生成隧道详情中的 Peer 配置显示（用于服务端显示客户端配置）
  const generateDetailPeerConfig = (peerIndex) => {
    if (!selectedTunnel || !selectedTunnel.peers || selectedTunnel.peers.length <= peerIndex) {
      return '配置不可用';
    }

    const peer = selectedTunnel.peers[peerIndex];
    if (!selectedTunnel.address || !peer.public_key) {
      return '请先完善配置';
    }

    // 获取服务端的 Endpoint
    let serverEndpoint = '服务端地址未配置';
    if (selectedTunnel.server_endpoint) {
      serverEndpoint = selectedTunnel.listen_port ?
        `${selectedTunnel.server_endpoint}:${selectedTunnel.listen_port}` :
        `${selectedTunnel.server_endpoint}:51820`;
    } else {
      serverEndpoint = selectedTunnel.listen_port ?
        `<服务器IP或域名>:${selectedTunnel.listen_port}` :
        '<服务器IP或域名>:51820';
    }

    const serverAllowedIps = selectedTunnel.address || '10.0.0.0/24';
    const clientPrivateKey = peer.client_private_key || '<客户端私钥>';

    const clientConfig = `[Interface]
PrivateKey = ${clientPrivateKey}
Address = ${peer.allowed_ips}
DNS = 8.8.8.8, 8.8.4.4
MTU = 1420

[Peer]
PublicKey = ${selectedTunnel.public_key || '<服务端公钥>'}
${peer.preshared_key ? `PreSharedKey = ${peer.preshared_key}` : '# PreSharedKey = <预共享密钥，可选>'}
AllowedIPs = ${serverAllowedIps}
Endpoint = ${serverEndpoint}
PersistentKeepalive = 25`;

    return clientConfig;
  };

  // 生成隧道详情中的 Surge Peer 配置
  const generateSurgeDetailPeerConfig = (peerIndex) => {
    if (!selectedTunnel || !selectedTunnel.peers || selectedTunnel.peers.length <= peerIndex) {
      return '配置不可用';
    }

    const peer = selectedTunnel.peers[peerIndex];
    if (!selectedTunnel.address || !peer.public_key) {
      return '请先完善配置';
    }

    // 获取服务端的 Endpoint
    let serverEndpoint = '';
    if (selectedTunnel.server_endpoint) {
      serverEndpoint = selectedTunnel.listen_port ?
        `${selectedTunnel.server_endpoint}:${selectedTunnel.listen_port}` :
        `${selectedTunnel.server_endpoint}:51820`;
    } else {
      return '服务端地址未配置';
    }

    const serverAllowedIps = selectedTunnel.address || '10.0.0.0/24';
    const clientPrivateKey = peer.client_private_key || '';
    const tunnelName = selectedTunnel.name || 'wireguard';

    if (!clientPrivateKey) {
      return '客户端私钥未生成';
    }

    const surgeConfig = `[Proxy]
wireguard-${tunnelName.replace(/\\s+/g, '')} = wireguard, section-name = WireGuard-${tunnelName.replace(/\\s+/g, '')}, underlying-proxy = direct

[WireGuard-${tunnelName.replace(/\\s+/g, '')}]
private-key = ${clientPrivateKey}
self-ip = ${peer.allowed_ips.split('/')[0]}
dns-server = 8.8.8.8, 8.8.4.4
mtu = 1420
peer = (public-key = ${selectedTunnel.public_key || ''}, allowed-ips = ${serverAllowedIps}, endpoint = ${serverEndpoint}${peer.preshared_key ? `, pre-shared-key = ${peer.preshared_key}` : ''})`;

    return surgeConfig;
  };

  // 生成 Peer 的二维码
  const generatePeerQrcode = async (peerIndex) => {
    try {
      const config = generateDetailPeerConfig(peerIndex);
      // 后端已返回完整的 Data URL，直接使用
      const dataUrl = await invoke('generate_qrcode', { content: config });
      return dataUrl;
    } catch (err) {
      console.error('生成二维码失败:', err);
      return null;
    }
  };

  // 复制 Peer 配置到剪贴板
  const handleCopyPeerConfig = async (peerIndex, configType = 'wireguard') => {
    try {
      let config;
      if (configType === 'wireguard') {
        config = generateDetailPeerConfig(peerIndex);
      } else if (configType === 'surge') {
        config = generateSurgeDetailPeerConfig(peerIndex);
      }
      await navigator.clipboard.writeText(config);
      onShowToast('配置已复制到剪贴板', 'success');
    } catch (err) {
      onShowToast('复制失败: ' + err, 'error');
    }
  };

  // 保存 Peer 配置文件
  const handleSavePeerConfig = async (peerIndex, configType = 'wireguard') => {
    try {
      let defaultPath, filters, config;

      if (configType === 'wireguard') {
        defaultPath = `peer_${peerIndex + 1}.conf`;
        filters = [{ name: 'WireGuard 配置', extensions: ['conf'] }];
        config = generateDetailPeerConfig(peerIndex);
      } else if (configType === 'surge') {
        defaultPath = `peer_${peerIndex + 1}_surge.conf`;
        filters = [{ name: 'Surge 配置', extensions: ['conf'] }];
        config = generateSurgeDetailPeerConfig(peerIndex);
      }

      const filePath = await save({
        defaultPath,
        filters
      });

      if (filePath) {
        await invoke("save_config_to_path", { content: config, filePath });
        onShowToast('配置文件已保存', 'success');
      }
    } catch (err) {
      onShowToast('保存失败: ' + err, 'error');
    }
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
      if (!config.mode) {
        onShowToast('请选择运行模式', 'warning');
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

      // 服务端必须配置公网地址
      if (config.mode === 'server' && !config.serverEndpoint) {
        onShowToast('请输入服务端地址 (公网 IP 或域名)', 'warning');
        return;
      }

      // 验证 Peer 配置
      if (config.peers.length === 0) {
        onShowToast(config.mode === 'server' ? '请至少添加一个 Peer' : '请配置要连接的服务端', 'warning');
        return;
      }

      for (let i = 0; i < config.peers.length; i++) {
        const peer = config.peers[i];
        if (!peer.publicKey) {
          const peerLabel = config.mode === 'server' ? `Peer ${i + 1}` : '服务端';
          onShowToast(`${peerLabel}: 请输入公钥`, 'warning');
          return;
        }
        if (!peer.allowedIps) {
          const peerLabel = config.mode === 'server' ? `Peer ${i + 1}` : '服务端';
          onShowToast(`${peerLabel}: 请输入 AllowedIPs`, 'warning');
          return;
        }
        // 客户端模式必须配置 Endpoint
        if (config.mode === 'client' && !peer.endpoint) {
          onShowToast('请输入服务端地址 (Endpoint)', 'warning');
          return;
        }
      }

      setLoading(true);

      // 构建要保存的配置对象
      const tunnelConfig = {
        id: editingConfig ? editingConfig.id : Date.now().toString(),
        name: config.name,
        mode: config.mode, // 保存模式信息
        private_key: config.privateKey,
        address: config.address,
        listen_port: String(config.listenPort || ''), // 确保是字符串
        dns: config.dns || '',
        mtu: String(config.mtu || '1420'), // 确保是字符串
        server_endpoint: config.serverEndpoint || '', // 服务端的公网地址
        peers: config.peers.map(peer => ({
          public_key: peer.publicKey,
          client_private_key: peer.clientPrivateKey || null, // 保存客户端的临时私钥
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
      mode: '',
      privateKey: '',
      address: '',
      listenPort: '',
      dns: '',
      mtu: '1420',
      serverEndpoint: '', // 重置服务端公网地址
      peers: [],
    });
    setLocalPublicKey('');
    setEditingConfig(null);
    setShowModeSelector(false);
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
            clientPrivateKey: p.client_private_key || '', // 加载保存的客户端私钥
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
          clientPrivateKey: '',
          presharedKey: fullConfig.preshared_key || '',
          endpoint: fullConfig.endpoint || '',
          allowedIps: fullConfig.allowed_ips || '0.0.0.0/0',
          persistentKeepalive: parseInt(fullConfig.persistent_keepalive) || 25,
        });
      }

      setConfig({
        name: fullConfig.name,
        mode: fullConfig.mode || 'server', // 默认为 server 模式
        privateKey: fullConfig.private_key || '',
        address: fullConfig.address || '',
        listenPort: fullConfig.listen_port || '',
        dns: fullConfig.dns || '',
        mtu: fullConfig.mtu || '1420',
        serverEndpoint: fullConfig.server_endpoint || '', // 加载服务端公网地址
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
      console.error(error);
      onShowToast(error, 'error');
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
      setActivePeerTab(0); // 重置 Peer Tab 为第一个
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


  return (
    <div className="tunnel-management-view">

      <div className="tunnel-actions">
        <button
          onClick={() => {
            resetForm();
            setShowModeSelector(true);
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


      {tunnels.length === 0 ? (
        <div className="empty-state">
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
                {!editingConfig && (
                  <div className="form-group">
                    <label>运行模式 *</label>
                    <div className="mode-display">
                      <span className="mode-badge" data-mode={config.mode}>
                        {config.mode === 'server' ? '🖥️ 服务端' : config.mode === 'client' ? '💻 客户端' : '未选择'}
                      </span>
                      <small>创建后无法修改模式，请谨慎选择</small>
                    </div>
                  </div>
                )}
                {editingConfig && (
                  <div className="form-group">
                    <label>运行模式</label>
                    <div className="mode-display">
                      <span className="mode-badge" data-mode={config.mode}>
                        {config.mode === 'server' ? '🖥️ 服务端' : config.mode === 'client' ? '💻 客户端' : '🖥️ 服务端'}
                      </span>
                    </div>
                  </div>
                )}
              </div>

              {/* Interface 配置 */}
              <div className="config-section">
                <h4>Interface (本机)</h4>

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
                  <div className="form-group public-key-display">
                    <label className="public-key-display-label">
                      <span>📢 本地公钥 (提供给对端)</span>
                      <button
                        onClick={handleCopyPublicKey}
                        className="btn-inline public-key-display-btn"
                        type="button"
                      >
                        📋 复制
                      </button>
                    </label>
                    <div className="public-key-display-value">
                      {localPublicKey}
                    </div>
                    <small className="public-key-display-hint">
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

                {/* 服务端特定的配置 */}
                {config.mode === 'server' && (
                  <div className="form-group">
                    <label>服务端地址 (公网 IP 或域名) *</label>
                    <input
                      type="text"
                      value={config.serverEndpoint || ''}
                      onChange={(e) => setConfig({ ...config, serverEndpoint: e.target.value })}
                      placeholder="例如: vpn.example.com 或 123.45.67.89"
                    />
                    <small>📝 用于客户端连接，生成的客户端配置会自动带入此地址，请输入公网 IP 或域名</small>
                  </div>
                )}
              </div>

              {/* Peer 配置 - 根据模式显示不同的 UI */}
              {config.mode && (
                <div className="config-section">
                  <div className="peer-section-header">
                    <div className="peer-section-header-content">
                      <h4>Peer (对端配置)</h4>
                      <small>
                        {config.mode === 'server'
                          ? '作为服务端时，需要预先配置 Peer，以建立加密隧道并验证客户端身份'
                          : config.mode === 'client'
                          ? '作为客户端时，配置要连接的服务端信息'
                          : ''}
                      </small>
                    </div>
                    {config.mode === 'server' && (
                      <div style={{ display: 'flex', gap: '0.5rem' }}>
                        <button
                          onClick={handleQuickAddClient}
                          className="btn-inline"
                          type="button"
                          title="一键生成客户端密钥对并自动配置 IP"
                          style={{ background: '#28a745' }}
                        >
                          ⚡ 快速添加客户端
                        </button>
                        <button
                          onClick={handleAddPeer}
                          className="btn-inline"
                          type="button"
                          title="手动添加 Peer 配置"
                        >
                          + 手动添加
                        </button>
                      </div>
                    )}
                  </div>

                {/* 服务端模式：支持多个 Peer */}
                {config.mode === 'server' && (
                  <>
                    {config.peers.length === 0 ? (
                      <div className="peer-empty-state">
                        <p>暂无 Peer 配置</p>
                        <small>点击"添加 Peer"按钮添加对端配置</small>
                      </div>
                    ) : (
                      config.peers.map((peer, index) => (
                        <div key={index} className="peer-config-group">
                          <div className="peer-config-header">
                            <h5>客户端 {index + 1}</h5>
                            <div style={{ display: 'flex', gap: '0.5rem' }}>
                              <button
                                onClick={() => {
                                  setPreviewPeerIndex(index);
                                  setShowClientPreview(true);
                                }}
                                className="btn-inline"
                                type="button"
                                title="预览客户端配置"
                                style={{ fontSize: '0.8rem', padding: '0.4rem 0.8rem' }}
                              >
                                👁️ 预览
                              </button>
                              <button
                                onClick={() => handleRemovePeer(index)}
                                className="btn-danger-outline peer-config-delete-btn"
                                type="button"
                              >
                                删除
                              </button>
                            </div>
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
                            <label>允许的 IP (AllowedIPs) *</label>
                            <input
                              type="text"
                              value={peer.allowedIps}
                              onChange={(e) => handleUpdatePeer(index, 'allowedIps', e.target.value)}
                              placeholder="0.0.0.0/0"
                            />
                            <small>客户端的 VPN IP 地址段</small>
                          </div>
                        </div>
                      ))
                    )}
                  </>
                )}

                {/* 客户端模式：单个 Peer */}
                {config.mode === 'client' && (
                  <>
                    {config.peers.length === 0 ? (
                      <div className="peer-empty-state">
                        <p>暂无服务端配置</p>
                        <small>点击下方"添加服务端"按钮配置要连接的服务端</small>
                      </div>
                    ) : (
                      <div className="peer-config-group">
                        <div className="peer-config-header">
                          <h5>连接的服务端</h5>
                          {config.peers.length > 0 && (
                            <button
                              onClick={() => handleRemovePeer(0)}
                              className="btn-danger-outline peer-config-delete-btn"
                              type="button"
                            >
                              删除
                            </button>
                          )}
                        </div>

                        <div className="form-group">
                          <label>服务端公钥 *</label>
                          <input
                            type="text"
                            value={config.peers[0]?.publicKey || ''}
                            onChange={(e) => handleUpdatePeer(0, 'publicKey', e.target.value)}
                            placeholder="输入服务端的公钥"
                            className="monospace-input"
                          />
                        </div>

                        <div className="form-group">
                          <label>预共享密钥 (可选)</label>
                          <div className="input-with-button">
                            <input
                              type="text"
                              value={config.peers[0]?.presharedKey || ''}
                              onChange={(e) => handleUpdatePeer(0, 'presharedKey', e.target.value)}
                              placeholder="点击生成或手动输入"
                              className="monospace-input"
                            />
                            <button
                              onClick={() => handleGeneratePresharedKey(0)}
                              className="btn-inline"
                              type="button"
                            >
                              生成 PSK
                            </button>
                          </div>
                        </div>

                        <div className="form-group">
                          <label>服务端地址 (Endpoint) *</label>
                          <input
                            type="text"
                            value={config.peers[0]?.endpoint || ''}
                            onChange={(e) => handleUpdatePeer(0, 'endpoint', e.target.value)}
                            placeholder="例如: vpn.example.com:51820"
                          />
                          <small>格式: 域名或IP:端口</small>
                        </div>

                        <div className="form-group">
                          <label>允许的 IP (AllowedIPs) *</label>
                          <input
                            type="text"
                            value={config.peers[0]?.allowedIps || '0.0.0.0/0'}
                            onChange={(e) => handleUpdatePeer(0, 'allowedIps', e.target.value)}
                            placeholder="0.0.0.0/0"
                          />
                          <small>0.0.0.0/0 表示通过此 VPN 路由所有流量</small>
                        </div>

                        <div className="form-group">
                          <label>保持连接 (PersistentKeepalive)</label>
                          <input
                            type="number"
                            value={config.peers[0]?.persistentKeepalive || 25}
                            onChange={(e) => handleUpdatePeer(0, 'persistentKeepalive', parseInt(e.target.value) || 0)}
                            placeholder="25"
                          />
                          <small>NAT 穿透保持连接间隔(秒), 建议设置为 25 秒</small>
                        </div>
                      </div>
                    )}
                    {config.peers.length === 0 && (
                      <button
                        onClick={handleAddPeer}
                        className="btn-primary"
                        style={{ width: '100%', marginTop: '1rem' }}
                        type="button"
                      >
                        + 添加服务端
                      </button>
                    )}
                  </>
                )}
                </div>
              )}
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
                <label>运行模式:</label>
                <div>
                  <span className="mode-badge" data-mode={selectedTunnel.mode || 'server'}>
                    {selectedTunnel.mode === 'server' ? '🖥️ 服务端' : selectedTunnel.mode === 'client' ? '💻 客户端' : '🖥️ 服务端'}
                  </span>
                </div>
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

              {/* Peer 列表 */}
              {selectedTunnel.peers && selectedTunnel.peers.length > 0 && (
                <div className="peer-list-section">
                  <h4>Peer 列表</h4>

                  {/* Peer Tab 导航 */}
                  <div className="tabs-nav">
                    {selectedTunnel.peers.map((peer, index) => (
                      <button
                        key={index}
                        className={`tab-button ${activePeerTab === index ? 'active' : ''}`}
                        onClick={() => setActivePeerTab(index)}
                      >
                        Peer {index + 1}
                      </button>
                    ))}
                  </div>

                  {/* Peer 配置内容 */}
                  <div className="tabs-content">
                    {selectedTunnel.peers.map((peer, index) => (
                      activePeerTab === index && (
                        <div key={index} className="tab-panel">
                          {/* 配置类型 Tab */}
                          <div className="config-type-tabs">
                            <button
                              className={`config-type-btn ${activePeerConfigTab === 'wireguard' ? 'active' : ''}`}
                              onClick={() => setActivePeerConfigTab('wireguard')}
                            >
                              WireGuard
                            </button>
                            <button
                              className={`config-type-btn ${activePeerConfigTab === 'qrcode' ? 'active' : ''}`}
                              onClick={() => setActivePeerConfigTab('qrcode')}
                            >
                              二维码
                            </button>
                            <button
                              className={`config-type-btn ${activePeerConfigTab === 'surge' ? 'active' : ''}`}
                              onClick={() => setActivePeerConfigTab('surge')}
                            >
                              Surge
                            </button>
                          </div>

                          {/* WireGuard 配置 */}
                          {activePeerConfigTab === 'wireguard' && (
                            <div className="config-result">
                              <div className="config-header">
                                <h5>WireGuard 配置 (Peer {index + 1})</h5>
                                <div className="button-group-inline">
                                  <button
                                    onClick={() => handleCopyPeerConfig(index, 'wireguard')}
                                    className="btn-save"
                                  >
                                    📋 复制
                                  </button>
                                  <button
                                    onClick={() => handleSavePeerConfig(index, 'wireguard')}
                                    className="btn-save"
                                  >
                                    💾 下载
                                  </button>
                                </div>
                              </div>
                              <pre className="config-content">{generateDetailPeerConfig(index)}</pre>
                            </div>
                          )}

                          {/* 二维码 */}
                          {activePeerConfigTab === 'qrcode' && (
                            <PeerQrcodeDisplay peerIndex={index} generateQrcode={generatePeerQrcode} />
                          )}

                          {/* Surge 配置 */}
                          {activePeerConfigTab === 'surge' && (
                            <div className="config-result">
                              <div className="config-header">
                                <h5>Surge 配置 (Peer {index + 1})</h5>
                                <div className="button-group-inline">
                                  <button
                                    onClick={() => handleCopyPeerConfig(index, 'surge')}
                                    className="btn-save"
                                  >
                                    📋 复制
                                  </button>
                                  <button
                                    onClick={() => handleSavePeerConfig(index, 'surge')}
                                    className="btn-save"
                                  >
                                    💾 下载
                                  </button>
                                </div>
                              </div>
                              <pre className="config-content">{generateSurgeDetailPeerConfig(index)}</pre>
                            </div>
                          )}
                        </div>
                      )
                    ))}
                  </div>
                </div>
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

{/* 隧道模式选择对话框 */}
      {showModeSelector && (
        <div className="modal-overlay" onClick={() => setShowModeSelector(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()} style={{ maxWidth: '500px' }}>
            <div className="modal-header">
              <h3>选择隧道运行模式</h3>
              <button
                onClick={() => setShowModeSelector(false)}
                className="btn-close"
              >
                ✕
              </button>
            </div>
            <div className="modal-body">
              <p style={{ marginBottom: '2rem', color: '#666', fontSize: '0.95rem' }}>
                请选择隧道的运行模式。创建后无法修改，请谨慎选择。
              </p>
              <div className="mode-selector">
                <button
                  onClick={() => {
                    setConfig({ ...config, mode: 'server' });
                    setShowModeSelector(false);
                    setShowConfigForm(true);
                  }}
                  className="mode-option-btn"
                >
                  <div className="mode-option-icon">🖥️</div>
                  <div className="mode-option-title">服务端模式</div>
                  <div className="mode-option-desc">作为 VPN 服务端，管理多个客户端连接</div>
                </button>
                <button
                  onClick={() => {
                    setConfig({ ...config, mode: 'client' });
                    setShowModeSelector(false);
                    setShowConfigForm(true);
                  }}
                  className="mode-option-btn"
                >
                  <div className="mode-option-icon">💻</div>
                  <div className="mode-option-title">客户端模式</div>
                  <div className="mode-option-desc">作为 VPN 客户端，连接到一个服务端</div>
                </button>
              </div>
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

      {/* 守护进程管理面板 (仅 Linux) */}
      {isLinux && (
        <DaemonPanel
          isOpen={showDaemonPanel}
          onClose={() => setShowDaemonPanel(false)}
          onShowToast={onShowToast}
        />
      )}
    </div>
  );
}

// Peer 二维码显示组件
function PeerQrcodeDisplay({ peerIndex, generateQrcode }) {
  const [qrcodeUrl, setQrcodeUrl] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    const loadQrcode = async () => {
      setLoading(true);
      setError(null);
      try {
        const url = await generateQrcode(peerIndex);
        if (url) {
          setQrcodeUrl(url);
        } else {
          setError('二维码生成失败');
        }
      } catch (err) {
        console.error('加载二维码出错:', err);
        setError('加载二维码出错: ' + err);
      } finally {
        setLoading(false);
      }
    };

    if (peerIndex !== undefined && generateQrcode) {
      loadQrcode();
    }
  }, [peerIndex, generateQrcode]);

  if (loading) {
    return (
      <div className="config-result">
        <div className="config-content" style={{ textAlign: 'center', padding: '2rem' }}>
          生成二维码中...
        </div>
      </div>
    );
  }

  if (error || !qrcodeUrl) {
    return (
      <div className="config-result">
        <div className="config-content" style={{ textAlign: 'center', padding: '2rem', color: '#d32f2f' }}>
          {error || '二维码生成失败，请使用其他配置方式'}
        </div>
      </div>
    );
  }

  return (
    <div className="config-result">
      <div className="qrcode-container">
        <h4>扫码快速导入</h4>
        <img src={qrcodeUrl} alt="WireGuard 配置二维码" className="qrcode" />
        <p className="qrcode-hint">使用 WireGuard 客户端扫描二维码即可快速导入配置</p>
        <div className="hint-box" style={{ marginTop: '1rem' }}>
          💡 支持 iOS、Android 等移动设备的 WireGuard 官方客户端
        </div>
      </div>
    </div>
  );
}

export default TunnelManagementView;
