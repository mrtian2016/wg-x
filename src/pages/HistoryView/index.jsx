import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useState, useEffect } from "react";
import HistoryDetailModal from "../../components/HistoryDetailModal";
import "./style.css";

function HistoryView({
  historyList,
  onDeleteHistory,
  onClearCache,
  onExportAllPeers,
  onExportAllZip,
  onShowToast,
  onBack,
}) {
  const [serverList, setServerList] = useState([]);
  const [selectedServerId, setSelectedServerId] = useState("");

  // 弹窗相关状态
  const [showModal, setShowModal] = useState(false);
  const [selectedHistory, setSelectedHistory] = useState(null);
  const [modalActiveTab, setModalActiveTab] = useState("wireguard");

  // 加载服务端列表
  useEffect(() => {
    const loadServers = async () => {
      try {
        const list = await invoke("get_server_list");
        setServerList(list);
      } catch (err) {
        console.error("加载服务端列表失败:", err);
      }
    };
    loadServers();
  }, []);

  // 获取筛选后的历史记录
  const filteredHistoryList = selectedServerId
    ? historyList.filter(item => item.server_id === selectedServerId)
    : historyList;

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
      onShowToast("加载历史详情失败: " + err, "error");
    }
  };

  return (
    <div className="form-section">
      <div className="history-header">
        <h2>📜 历史记录</h2>
        <button onClick={onBack} className="btn-secondary" style={{ fontSize: "0.9rem" }}>
          ← 返回
        </button>
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
              <button onClick={onClearCache} className="btn-primary" style={{ fontSize: "0.8rem", padding: "0.3rem 0.6rem" }}>
                🧹 清空历史记录
              </button>
              {historyList.length > 0 && (
                <>
                  <button onClick={onExportAllZip} className="btn-generate" style={{ fontSize: "0.8rem", padding: "0.3rem 0.6rem" }}>
                    📦 导出 ZIP
                  </button>
                  <button onClick={onExportAllPeers} className="btn-generate" style={{ fontSize: "0.8rem", padding: "0.3rem 0.6rem" }}>
                    📤 导出 Peers
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
                    <strong className="history-card-title">{item.ikuai_comment}</strong>
                    <span className="history-card-id">
                      (ID: {item.ikuai_id})
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
                      onDeleteHistory(item.id);
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
              onShowToast={onShowToast}
            />
          )}
        </>
      )}
    </div>
  );
}

export default HistoryView;
