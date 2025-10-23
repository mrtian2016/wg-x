import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "./hooks/useToast";
import Toast from "./components/Toast";
import ConfirmDialog from "./components/ConfirmDialog";
import HistoryView from "./pages/HistoryView";
import ServerManagementView from "./pages/ServerManagementView";
import WebDavSettingsView from "./pages/WebDavSettingsView";
import TunnelManagementView from "./pages/TunnelManagementView";
import UpdateProgressDialog from "./components/UpdateProgressDialog";
import ConfigGeneratorView from "./pages/ConfigGeneratorView";
import { updateManager } from "./utils/updateManager";
import "./styles/App.css";

function App() {
  const { messages, showToast, removeToast } = useToast();
  // 基本配置
  const [interfaceName, setInterfaceName] = useState("wg0");
  const [privateKey, setPrivateKey] = useState("");
  const [publicKey, setPublicKey] = useState("");
  const [address, setAddress] = useState("");
  const [listenPort, setListenPort] = useState("");
  const [dns, setDns] = useState("");

  // 对端配置（持久化）
  const [peerPublicKey, setPeerPublicKey] = useState("");
  const [presharedKey, setPresharedKey] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [allowedIps, setAllowedIps] = useState("0.0.0.0/0,::/0");
  const [keepalive, setKeepalive] = useState("25");

  // 爱快配置（持久化）
  const [peerId, setpeerId] = useState(1);
  const [peerInterface, setpeerInterface] = useState("wg_0");
  const [peerComment, setpeerComment] = useState("");

  // UI 状态
  const [step, setStep] = useState(0);
  const [loading, setLoading] = useState(false);
  const [wgConfigContent, setWgConfigContent] = useState("");
  const [surgeConfigContent, setSurgeConfigContent] = useState("");
  const [mikrotikConfigContent, setMikrotikConfigContent] = useState("");
  const [openwrtConfigContent, setOpenwrtConfigContent] = useState("");
  const [qrcodeDataUrl, setQrcodeDataUrl] = useState("");
  const [workDir, setWorkDir] = useState("");

  // 累积的 peer 配置列表
  const [allPeerConfigs, setAllPeerConfigs] = useState([]);

  // 服务端相关状态
  const [selectedServerId, setSelectedServerId] = useState("");  // 当前选择的服务端ID
  const [selectedServerName, setSelectedServerName] = useState("");  // 当前选择的服务端名称
  const [serverList, setServerList] = useState([]);  // 服务端列表
  const [showServerManagement, setShowServerManagement] = useState(false);  // 是否显示服务端管理界面

  // 历史记录相关状态
  const [showHistory, setShowHistory] = useState(false);
  const [historyList, setHistoryList] = useState([]);

  // WebDAV 设置相关状态
  const [showWebDavSettings, setShowWebDavSettings] = useState(false);

  // 隧道管理相关状态
  const [showTunnelManagement, setShowTunnelManagement] = useState(false);

  // 主视图状态: 'tunnel'(隧道列表/首页), 'config'(配置生成), 'server'(服务端管理), 'history'(历史记录), 'webdav'(WebDAV设置)
  const [currentPage, setCurrentPage] = useState('tunnel');

  const [webdavConfig, setWebdavConfig] = useState({
    enabled: false,
    server_url: '',
    username: '',
    password: '',
    sync_interval: 300,
    auto_sync_enabled: false,
  });

  // 确认对话框状态
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const [confirmDialogConfig, setConfirmDialogConfig] = useState({
    title: "",
    message: "",
    onConfirm: () => {},
  });

  // 标签页状态
  const [activeTab, setActiveTab] = useState("wireguard"); // wireguard, qrcode, surge, ikuai, mikrotik, openwrt

  // 更新进度状态（从 updateManager 订阅）
  const [updateProgress, setUpdateProgress] = useState(updateManager.getProgress());

  // 订阅 updateManager 的进度更新
  useEffect(() => {
    const unsubscribe = updateManager.subscribe(setUpdateProgress);
    return unsubscribe;
  }, []);

  // 初始化：加载配置
  useEffect(() => {
    const init = async () => {
      try {
        // 尝试迁移旧配置
        try {
          const migratedServerId = await invoke("migrate_old_config_to_server");
          if (migratedServerId) {
            showToast("检测到旧版配置，已自动迁移为新服务端", "info");
            console.log("已迁移旧配置，新服务端ID:", migratedServerId);
          }
        } catch (err) {
          console.error("迁移旧配置失败:", err);
        }

        const dir = ".";
        setWorkDir(dir);

        // 加载环境变量
        const envConfig = await invoke("load_env_config", { workDir: dir });
        if (envConfig.interface_name) setInterfaceName(envConfig.interface_name);
        if (envConfig.listen_port) setListenPort(envConfig.listen_port);
        if (envConfig.dns_server) setDns(envConfig.dns_server);

        // 加载webdav配置
        loadWebDavConfig();
        // 注：旧的持久化配置加载逻辑已移除，现在使用服务端配置

        // 检查应用更新
        try {
          const update = await updateManager.checkForUpdates();
          if (update) {
            setConfirmDialogConfig({
              title: "🎉 发现新版本",
              message: `发现新版本 ${update.version}!\n\n当前版本: ${update.currentVersion}\n新版本: ${update.version}\n\n是否立即下载并安装更新？`,
              onConfirm: async () => {
                try {
                  await updateManager.downloadAndInstall(update);
                  showToast("更新已安装，请重启应用以完成更新", "success");
                } catch (err) {
                  showToast("更新失败: " + err, "error");
                }
              },
            });
            setShowConfirmDialog(true);
          }
        } catch (err) {
          console.error("检查更新失败:", err);
          // 静默失败，不影响正常使用
        }
      } catch (err) {
        console.error("初始化失败:", err);
      }
    };

    init();
  }, []);

  // 全局自动同步定时器 - 在任何页面都会运行
  useEffect(() => {
    let timer;
    if (webdavConfig.auto_sync_enabled && webdavConfig.enabled && webdavConfig.sync_interval > 0) {
      // 立即执行一次同步（如果启用了自动同步）
      console.log('启动自动同步定时器，间隔:', webdavConfig.sync_interval, '秒');

      timer = setInterval(() => {
        handleAutoSync();
      }, webdavConfig.sync_interval * 1000);
    }
    return () => {
      if (timer) {
        console.log('清理自动同步定时器');
        clearInterval(timer);
      }
    };
  }, [webdavConfig.auto_sync_enabled, webdavConfig.enabled, webdavConfig.sync_interval]);

  const handleAutoSync = async () => {
    try {
      console.log('执行自动同步...');
      const result = await invoke('sync_bidirectional_webdav');
      console.log('自动同步完成:', result);
    } catch (error) {
      console.error('自动同步失败:', error);
    }
  };


  const loadWebDavConfig = async () => {
    try {
      const loadedConfig = await invoke('load_webdav_config');
      setWebdavConfig(loadedConfig);
    } catch (error) {
      console.error('加载配置失败:', error);
    }
  };

  // 手动检查更新
  const handleCheckUpdate = async () => {
    try {
      setLoading(true);
      showToast("正在检查更新...", "info");
      const update = await updateManager.checkForUpdates();
      if (update) {
        setLoading(false);
        setConfirmDialogConfig({
          title: "🎉 发现新版本",
          message: `发现新版本 ${update.version}!\n\n当前版本: ${update.currentVersion}\n新版本: ${update.version}\n\n是否立即下载并安装更新？`,
          onConfirm: async () => {
            try {
              await updateManager.downloadAndInstall(update);
              showToast("更新已安装，请重启应用以完成更新", "warning");
            } catch (err) {
              showToast("更新失败: " + err, "error");
            }
          },
        });
        setShowConfirmDialog(true);
      } else {
        showToast("当前已是最新版本", "info");
        setLoading(false);
      }
    } catch (err) {
      console.error("检查更新失败:", err);
      showToast("检查更新失败: " + err, "error");
      setLoading(false);
    }
  };

  return (
    <div className="container">
      {/* Toast 消息通知 */}
      <Toast messages={messages} onRemove={removeToast} />

      <header>
        <div className="header-content">
          <h1>🔐 WireGuard X</h1>
        </div>
      </header>

      {/* 主容器：左侧导航 + 右侧内容 */}
      <div className="app-layout">
        {/* 左侧导航栏 */}
        <nav className="sidebar">
          {/* 隧道管理部分 */}
          <div className="sidebar-section">
            <button
              className={`nav-item ${currentPage === 'tunnel' ? 'active' : ''}`}
              onClick={() => setCurrentPage('tunnel')}
            >
              隧道管理
            </button>

            <button
              className={`nav-item ${currentPage === 'config' ? 'active' : ''}`}
              onClick={() => setCurrentPage('config')}
            >
              配置生成
            </button>
            <button
              className={`nav-item ${currentPage === 'server' ? 'active' : ''}`}
              onClick={() => setCurrentPage('server')}
            >
              服务管理
            </button>

            <button
              className={`nav-item ${currentPage === 'history' ? 'active' : ''}`}
              onClick={async () => {
                setCurrentPage('history');
              }}
            >
              历史记录
            </button>
            <button
              className={`nav-item ${currentPage === 'webdav' ? 'active' : ''}`}
              onClick={() => setCurrentPage('webdav')}
            >
              同步设置
            </button>
          </div>
        </nav>

        {/* 右侧主内容区域 */}
        <div className="main-content-wrapper">
          {/* 根据 currentPage 显示不同的页面 */}
          {currentPage === 'tunnel' ? (
            <TunnelManagementView
              onShowToast={showToast}
            />
          ) : currentPage === 'server' ? (
            <ServerManagementView
              onShowToast={showToast}
            />
          ) : currentPage === 'webdav' ? (
            <WebDavSettingsView
              onConfigChange={loadWebDavConfig}
            />
          ) : currentPage === 'history' ? (
            <HistoryView
              onSetConfirmDialogConfig={setConfirmDialogConfig}
              onSetShowConfirmDialog={setShowConfirmDialog}
            />
          ) : currentPage === 'config' ? (
            <ConfigGeneratorView

              onNavPage={() => setCurrentPage('server')}
              onShowToast={showToast}
            />
          ) : null}
        </div>
      </div>

      {/* 确认对话框 */}
      <ConfirmDialog
        isOpen={showConfirmDialog}
        title={confirmDialogConfig.title}
        message={confirmDialogConfig.message}
        onConfirm={() => {
          setShowConfirmDialog(false);
          confirmDialogConfig.onConfirm();
        }}
        onCancel={() => setShowConfirmDialog(false)}
      />

      {/* 更新进度对话框 */}
      <UpdateProgressDialog
        progress={updateProgress}
        onClose={() => {
          updateManager.closeProgress();
          showToast(updateProgress.status === "done" ? "请稍后手动重启应用以完成更新" : "已取消更新");
        }}
        onRestart={() => updateManager.restartApp()}
      />

      {/* Footer */}
      <footer className="app-footer">
        <div className="footer-content">
          <a
            href="https://github.com/mrtian2016/wg-x"
            target="_blank"
            rel="noopener noreferrer"
            className="footer-link"
          >
            <svg height="16" width="16" viewBox="0 0 16 16" fill="currentColor">
              <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
            </svg>
            GitHub
          </a>
          <span className="footer-version">当前版本: v{__APP_VERSION__}</span>
          <button
            onClick={handleCheckUpdate}
            disabled={loading}
            className="footer-button"
          >
            {loading ? "检查中..." : "检查更新"}
          </button>
        </div>
      </footer>
    </div>
  );
}

export default App;
