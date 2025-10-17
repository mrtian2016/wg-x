import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import "./style.css";
import ConfigTabs from "../ConfigTabs";

function HistoryDetailModal({
  history,
  activeTab,
  onSetActiveTab,
  onClose,
  onShowToast,
}) {
  if (!history) return null;


  // 保存配置的函数
  const handleSaveConfig = async (content, defaultFileName, filterName, extensions) => {
    try {
      const filePath = await save({
        defaultPath: defaultFileName,
        filters: [{ name: filterName, extensions: extensions }]
      });

      if (filePath) {
        await invoke("save_config_to_path", { content, filePath });
        onShowToast("配置已保存", "success");
      }
    } catch (err) {
      onShowToast("保存失败: " + err, "error");
    }
  };

  // 构造传递给 ConfigTabs 的属性
  const configTabsProps = {
    activeTab,
    onSetActiveTab,
    interfaceName: history.interface_name,
    wgConfigContent: history.wg_config,
    qrcodeDataUrl: history.qrcode,
    surgeConfigContent: history.surge_config,
    allPeerConfigs: [history.ikuai_config], // 将爱快配置作为数组传递
    mikrotikConfigContent: history.mikrotik_config,
    openwrtConfigContent: history.openwrt_config,
    publicKey: history.public_key,
    onShowToast,
    onSavePeerConfig: async () => {
      await handleSaveConfig(
        history.ikuai_config,
        `${history.ikuai_comment}_peer.txt`,
        'Peer 配置',
        ['txt']
      );
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h3>{history.ikuai_comment} 配置详情</h3>
          <button className="modal-close-btn" onClick={onClose}>
            ✕
          </button>
        </div>

        <div className="modal-body">
          

          {/* 使用 ConfigTabs 组件渲染标签页内容 */}
          <div className="tabs-content">
            <ConfigTabs {...configTabsProps} />
          </div>
        </div>

        <div className="modal-footer">
          <button onClick={onClose} className="btn-primary">
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}

export default HistoryDetailModal;
