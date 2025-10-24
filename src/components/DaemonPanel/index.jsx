import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import ConfirmDialog from '../ConfirmDialog';
import './style.css';

function DaemonPanel({ isOpen, onClose, onShowToast }) {
  const [daemonStatus, setDaemonStatus] = useState(null);
  const [daemonLogs, setDaemonLogs] = useState('');
  const [loading, setLoading] = useState(false);
  const [confirmDialog, setConfirmDialog] = useState({
    isOpen: false,
    title: '',
    message: '',
    onConfirm: () => {},
  });

  // 加载守护进程状态
  const loadDaemonStatus = async () => {
    try {
      const status = await invoke('check_daemon_status');
      setDaemonStatus(status);
    } catch (error) {
      console.error('获取守护进程状态失败:', error);
    }
  };

  // 初始加载状态
  useEffect(() => {
    if (isOpen) {
      loadDaemonStatus();
      // 每 2 秒刷新一次状态
      const interval = setInterval(loadDaemonStatus, 2000);
      return () => clearInterval(interval);
    }
  }, [isOpen]);

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

  if (!isOpen) return null;

  return (
    <div className="daemon-panel-overlay" >
      <div className="daemon-panel-modal" onClick={(e) => e.stopPropagation()}>
        <div className="daemon-panel-header">
          <h3>Linux 守护进程管理</h3>
          <button onClick={onClose} className="btn-close">
            ✕
          </button>
        </div>

        <div className="daemon-panel-body">
          {/* 状态信息 */}
          {daemonStatus && (
            <div className="daemon-status-grid">
              <div className="daemon-status-item">
                <strong>安装状态:</strong> {daemonStatus.installed ? '✓ 已安装' : '✗ 未安装'}
              </div>
              <div className="daemon-status-item">
                <strong>运行状态:</strong> {daemonStatus.running ? '🟢 运行中' : '🔴 已停止'}
              </div>
              <div className="daemon-status-item">
                <strong>开机自启:</strong> {daemonStatus.enabled ? '✓ 已启用' : '✗ 未启用'}
              </div>
              {daemonStatus.version && (
                <div className="daemon-status-item">
                  <strong>版本:</strong> {daemonStatus.version}
                </div>
              )}
            </div>
          )}

          {/* 操作按钮 */}
          {daemonStatus && (
            <div className="daemon-actions">
              {!daemonStatus.installed ? (
                <button onClick={handleInstallDaemon} className="btn-primary" disabled={loading}>
                  📦 安装守护进程
                </button>
              ) : (
                <>
                  <div className="daemon-actions-row">
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
                  <div className="daemon-actions-row">
                    <button onClick={handleEnableDaemon} className="btn-secondary" disabled={loading || daemonStatus.enabled}>
                      ✓ 启用开机自启
                    </button>
                    <button onClick={handleDisableDaemon} className="btn-secondary" disabled={loading || !daemonStatus.enabled}>
                      ✗ 禁用开机自启
                    </button>
                    <button onClick={handleUninstallDaemon} className="btn-danger-outline daemon-uninstall-btn" disabled={loading}>
                      🗑️ 卸载守护进程
                    </button>
                  </div>
                </>
              )}
            </div>
          )}

          {/* 日志显示 */}
          {daemonLogs && (
            <div className="daemon-logs-container">
              <div className="daemon-logs-header">
                <h4>日志 (最近 100 行)</h4>
                <button onClick={() => setDaemonLogs('')} className="btn-secondary btn-sm">
                  关闭日志
                </button>
              </div>
              <pre className="daemon-logs-content">
                {daemonLogs}
              </pre>
            </div>
          )}
        </div>

        {/* 确认对话框 */}
        <ConfirmDialog
          isOpen={confirmDialog.isOpen}
          title={confirmDialog.title}
          message={confirmDialog.message}
          onConfirm={confirmDialog.onConfirm}
          onCancel={() => setConfirmDialog({ ...confirmDialog, isOpen: false })}
        />
      </div>
    </div>
  );
}

export default DaemonPanel;
