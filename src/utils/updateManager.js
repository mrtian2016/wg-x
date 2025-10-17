import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * 更新管理器
 * 负责应用的更新检查、下载和安装
 */
export class UpdateManager {
  constructor() {
    this.updateProgress = {
      show: false,
      downloaded: 0,
      total: 0,
      status: "" // "downloading", "installing", "done"
    };
    this.listeners = new Set();
  }

  /**
   * 订阅更新进度变化
   * @param {Function} listener 监听器函数
   * @returns {Function} 取消订阅函数
   */
  subscribe(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * 通知所有监听器
   */
  notify() {
    this.listeners.forEach(listener => listener(this.updateProgress));
  }

  /**
   * 更新进度状态
   * @param {Object} newProgress 新的进度状态
   */
  setProgress(newProgress) {
    this.updateProgress = { ...this.updateProgress, ...newProgress };
    this.notify();
  }

  /**
   * 检查更新
   * @returns {Promise<Object|null>} 更新信息或 null（无更新）
   */
  async checkForUpdates() {
    try {
      const update = await check();
      return update;
    } catch (error) {
      console.error("检查更新失败:", error);
      throw error;
    }
  }

  /**
   * 下载并安装更新
   * @param {Object} update 更新对象
   * @param {Function} onProgress 进度回调（可选）
   */
  async downloadAndInstall(update, onProgress) {
    try {
      this.setProgress({ show: true, downloaded: 0, total: 0, status: "downloading" });

      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          this.setProgress({
            show: true,
            downloaded: 0,
            total: event.data.contentLength,
            status: "downloading"
          });
        } else if (event.event === "Progress") {
          this.setProgress({
            downloaded: this.updateProgress.downloaded + event.data.chunkLength
          });
        } else if (event.event === "Finished") {
          this.setProgress({ status: "installing" });
        }

        // 调用外部进度回调
        if (onProgress) {
          onProgress(event);
        }
      });

      this.setProgress({ status: "done" });
      return true;
    } catch (error) {
      this.setProgress({ show: false, downloaded: 0, total: 0, status: "" });
      throw error;
    }
  }

  /**
   * 关闭更新进度对话框
   */
  closeProgress() {
    this.setProgress({ show: false, downloaded: 0, total: 0, status: "" });
  }

  /**
   * 重启应用
   */
  async restartApp() {
    try {
      await relaunch();
    } catch (error) {
      console.error("重启应用失败:", error);
      throw error;
    }
  }

  /**
   * 获取当前进度
   * @returns {Object} 当前进度对象
   */
  getProgress() {
    return { ...this.updateProgress };
  }

  /**
   * 格式化文件大小
   * @param {number} bytes 字节数
   * @returns {string} 格式化后的大小
   */
  static formatSize(bytes) {
    return (bytes / 1024 / 1024).toFixed(2) + " MB";
  }

  /**
   * 计算下载进度百分比
   * @param {number} downloaded 已下载字节数
   * @param {number} total 总字节数
   * @returns {string} 百分比字符串
   */
  static calculateProgress(downloaded, total) {
    if (total === 0) return "0.0";
    return ((downloaded / total) * 100).toFixed(1);
  }
}

// 导出单例实例
export const updateManager = new UpdateManager();
