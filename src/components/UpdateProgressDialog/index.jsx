import { UpdateManager } from "../../utils/updateManager";
import "./style.css";

/**
 * 更新进度对话框组件
 * 显示应用更新的下载和安装进度
 */
function UpdateProgressDialog({ progress, onClose, onRestart }) {
  if (!progress.show) return null;

  const { status, downloaded, total } = progress;
  const percentage = UpdateManager.calculateProgress(downloaded, total);
  const downloadedSize = UpdateManager.formatSize(downloaded);
  const totalSize = UpdateManager.formatSize(total);

  return (
    <div className="dialog-overlay">
      <div className="dialog-content progress-dialog">
        {/* 关闭按钮 - 仅在下载和完成状态显示 */}
        {(status === "downloading" || status === "done") && (
          <button
            onClick={onClose}
            className="dialog-close-btn"
            title="关闭"
          >
            ✕
          </button>
        )}

        <h3>
          {status === "downloading" && "⬇️ 正在下载更新"}
          {status === "installing" && "📦 正在安装更新"}
          {status === "done" && "✅ 更新完成"}
        </h3>

        {/* 下载进度条 */}
        {status === "downloading" && total > 0 && (
          <>
            <div className="progress-bar-container">
              <div
                className="progress-bar-fill"
                style={{ width: `${percentage}%` }}
              />
            </div>
            <div className="progress-info">
              <span className="progress-percentage">
                {percentage}%
              </span>
              <span className="progress-size">
                {downloadedSize} / {totalSize}
              </span>
            </div>
          </>
        )}

        {/* 安装中提示 */}
        {status === "installing" && (
          <div className="progress-message">
            <div className="spinner" />
            <p>正在安装更新，请稍候...</p>
          </div>
        )}

        {/* 完成后的操作按钮 */}
        {status === "done" && (
          <div className="progress-message">
            <p style={{ marginBottom: "1.5rem" }}>✅ 更新安装成功！</p>
            <div style={{ display: "flex", gap: "1rem", justifyContent: "center" }}>
              <button
                onClick={onClose}
                className="btn-secondary"
                style={{ padding: "0.75rem 1.5rem" }}
              >
                稍后重启
              </button>
              <button
                onClick={onRestart}
                className="btn-primary"
                style={{ padding: "0.75rem 1.5rem" }}
              >
                立即重启
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default UpdateProgressDialog;
