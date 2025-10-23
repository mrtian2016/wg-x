import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useState, useEffect } from "react";
import { useToast } from "../../hooks/useToast";
import HistoryDetailModal from "../../components/HistoryDetailModal";
import "./style.css";

function HistoryView({
  onSetConfirmDialogConfig,
  onSetShowConfirmDialog,
}) {
  const { messages, showToast, removeToast } = useToast();
  const [serverList, setServerList] = useState([]);
  const [historyList, setHistoryList] = useState([]);
  const [selectedServerId, setSelectedServerId] = useState("");

  // 弹窗相关状态
  const [showModal, setShowModal] = useState(false);
  const [selectedHistory, setSelectedHistory] = useState(null);
  const [modalActiveTab, setModalActiveTab] = useState("wireguard");

  // 加载服务端列表
  useEffect(() => {
    loadServers();
    loadHistoryList();
  }, []);

  const loadServers = async () => {
    try {
      const list = await invoke("get_server_list");
      setServerList(list);
    } catch (err) {
      console.error("加载服务端列表失败:", err);
    }
  };
  // 获取筛选后的历史记录
  const filteredHistoryList = selectedServerId
    ? historyList.filter(item => item.server_id === selectedServerId)
    : historyList;
  
  
  // 删除历史记录
  const handleDeleteHistory = async (id) => {
    try {
      await invoke("delete_history", { id });
      await loadHistoryList();
      showToast("历史记录已删除", "success");
    } catch (err) {
      showToast("删除失败: " + err, "error");
    }
  };

  // 导出所有 Peers 配置
  const handleExportAllPeers = async () => {
    try {
      if (historyList.length === 0) {
        showToast("没有可导出的历史记录", "warning");
        return;
      }

      // 获取所有历史记录的详细信息
      const allPeers = [];
      for (const item of historyList) {
        try {
          const detail = await invoke("get_history_detail", { id: item.id });
          allPeers.push(detail.ikuai_config);
        } catch (err) {
          console.error(`获取历史记录 ${item.id} 失败:`, err);
        }
      }

      if (allPeers.length === 0) {
        showToast("没有可导出的配置", "warning");
        return;
      }

      // 合并所有配置，每行一个
      const allContent = allPeers.join('\n');

      // 打开保存对话框
      const filePath = await save({
        defaultPath: 'all_peers.txt',
        filters: [{
          name: 'Peer 配置',
          extensions: ['txt']
        }]
      });

      if (filePath) {
        await invoke("save_config_to_path", { content: allContent, filePath });
        showToast(`已导出 ${allPeers.length} 条 Peer 配置`, "success");
      }
    } catch (err) {
      showToast("导出失败: " + err, "error");
    }
  };

  // 显示清空确认对话框
  const handleClearCache = () => {
    console.log("handleClearCache");
    onSetConfirmDialogConfig({
      title: "⚠️ 清空历史记录",
      message: `确定要清空所有历史记录吗？\n\n这会删除：\n• 所有历史记录（共 ${historyList.length} 条）\n\n注意：服务端配置不会被删除\n此操作不可恢复！`,
      onConfirm: confirmClearCache,
    });
    onSetShowConfirmDialog(true);
  };

  // 执行清空操作
  const confirmClearCache = async () => {
    try {
      // 只清空历史记录，不清空服务端配置
      await invoke("clear_all_history");

      // 清空历史记录状态
      setHistoryList([]);

      showToast("历史记录已清空", "success");
    } catch (err) {
      showToast("清空历史记录失败: " + err, "error");
    }
  };

  // 导出所有配置为 ZIP
  const handleExportAllZip = async () => {
    try {
      if (historyList.length === 0) {
        showToast("没有可导出的历史记录", "warning");
        return;
      }

      // 打开保存对话框
      const filePath = await save({
        defaultPath: 'wireguard-configs.zip',
        filters: [{
          name: 'ZIP 压缩包',
          extensions: ['zip']
        }]
      });

      if (filePath) {
        await invoke("export_all_configs_zip", { zipPath: filePath });
        showToast(`已导出 ${historyList.length} 条配置到 ZIP 文件`, "success");
      }
    } catch (err) {
      showToast("导出 ZIP 失败: " + err, "error");
    }
  };

  // 查看历史记录详情（打开弹窗）
  const handleViewHistory = async (id) => {
    try {
      const detail = await invoke("get_history_detail", { id });

      // 为历史配置生成二维码
      try {
        const qrcode = await invoke("generate_qrcode", { content: detail.wg_config });
        detail.qrcode = qrcode;
      } catch (err) {
        console.error("生成二维码失败:", err);
      }

      setSelectedHistory(detail);
      setModalActiveTab("wireguard");
      setShowModal(true);
    } catch (err) {
      showToast("加载历史详情失败: " + err, "error");
    }
  };

  // 加载历史记录列表
  const loadHistoryList = async (serverId = null) => {
    try {
      let list;
      if (serverId) {
        list = await invoke("get_history_list_by_server", { serverId });
      } else {
        list = await invoke("get_history_list");
      }
      setHistoryList(list);
    } catch (err) {
      console.error("加载历史记录失败:", err);
    }
  };
  
  return (
    <div className="form-section">
      <div className="history-header">
        <h2>历史记录</h2>
      </div>

      {historyList.length === 0 ? (
        <p className="history-hint history-hint-empty">
          暂无历史记录
        </p>
      ) : (
        <>
          {/* 服务端筛选 */}
          {serverList.length > 0 && (
            <div className="form-group history-filter">
              <label>按服务端筛选</label>
              <div className="custom-select">
                <select
                  value={selectedServerId}
                  onChange={(e) => setSelectedServerId(e.target.value)}
                >
                  <option value="">全部服务端</option>
                  {serverList.map(server => (
                    <option key={server.id} value={server.id}>
                      {server.name} ({server.endpoint})
                    </option>
                  ))}
                </select>
              </div>
            </div>
          )}

          <div className="history-actions">
            <p className="history-hint">
              共 {historyList.length} 条记录
              {selectedServerId && ` | 筛选后: ${filteredHistoryList.length} 条`}
            </p>
            <div className="history-actions-buttons">
              <button onClick={handleClearCache} className="btn-primary" style={{ fontSize: "0.8rem", padding: "0.3rem 0.6rem" }}>
                清空历史记录
              </button>
              {historyList.length > 0 && (
                <>
                  <button onClick={handleExportAllZip} className="btn-generate" style={{ fontSize: "0.8rem", padding: "0.3rem 0.6rem" }}>
                    导出 ZIP
                  </button>
                  <button onClick={handleExportAllPeers} className="btn-generate" style={{ fontSize: "0.8rem", padding: "0.3rem 0.6rem" }}>
                    导出 Peers
                  </button>
                </>
              )}
            </div>
          </div>

          <div className="history-list">
            {filteredHistoryList.map((item) => (
              <div
                key={item.id}
                className="history-card"
                onClick={() => handleViewHistory(item.id)}
              >
                <div className="history-card-header">
                  <div className="history-card-info">
                    <strong className="history-card-title">{item.peer_comment}</strong>
                    <span className="history-card-id">
                      (ID: {item.peer_id})
                    </span>
                    {item.server_name && (
                      <span className="history-card-server">
                        {item.server_name}
                      </span>
                    )}
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDeleteHistory(item.id);
                    }}
                    className="btn-secondary"
                    style={{ fontSize: "0.75rem", padding: "0.2rem 0.5rem" }}
                  >
                    删除
                  </button>
                </div>
                <div className="history-card-meta">
                  {item.interface_name} | {item.address} | {new Date(item.timestamp).toLocaleString()}
                </div>
              </div>
            ))}
          </div>

          {/* 历史记录详情弹窗 */}
          {showModal && selectedHistory && (
            <HistoryDetailModal
              history={selectedHistory}
              activeTab={modalActiveTab}
              onSetActiveTab={setModalActiveTab}
              onClose={() => {
                setShowModal(false);
                setSelectedHistory(null);
              }}
              showToast={showToast}
            />
          )}
        </>
      )}
    </div>
  );
}

export default HistoryView;
