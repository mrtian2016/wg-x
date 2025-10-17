import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./style.css";

function ConfigTabs({
  activeTab,
  onSetActiveTab,
  interfaceName,
  wgConfigContent,
  qrcodeDataUrl,
  surgeConfigContent,
  allPeerConfigs,
  mikrotikConfigContent,
  openwrtConfigContent,
  publicKey,
  onShowToast,
  onSavePeerConfig,
}) {
  // 复制到剪贴板
  const handleCopyToClipboard = async (content, name) => {
    try {
      await navigator.clipboard.writeText(content);
      onShowToast(`${name}已复制到剪贴板`, "success");
    } catch (err) {
      onShowToast("复制失败: " + err, "error");
    }
  };

  // 保存 WireGuard 配置文件
  const handleSaveWgConfig = async () => {
    try {
      const filePath = await save({
        defaultPath: `${interfaceName}.conf`,
        filters: [{
          name: 'WireGuard 配置',
          extensions: ['conf']
        }]
      });

      if (filePath) {
        await invoke("save_config_to_path", { content: wgConfigContent, filePath });
        onShowToast("配置文件已保存", "success");
      }
    } catch (err) {
      onShowToast("保存失败: " + err, "error");
    }
  };

  // 保存 Surge 配置文件
  const handleSaveSurgeConfig = async () => {
    try {
      const filePath = await save({
        defaultPath: `${interfaceName || 'surge'}.conf`,
        filters: [{
          name: 'Surge 配置',
          extensions: ['conf']
        }]
      });

      if (filePath) {
        await invoke("save_config_to_path", { content: surgeConfigContent, filePath });
        onShowToast("Surge 配置文件已保存", "success");
      }
    } catch (err) {
      onShowToast("保存失败: " + err, "error");
    }
  };

  // 保存 MikroTik 配置文件
  const handleSaveMikrotikConfig = async () => {
    try {
      const filePath = await save({
        defaultPath: `mikrotik_peer.rsc`,
        filters: [{
          name: 'MikroTik 脚本',
          extensions: ['rsc', 'txt']
        }]
      });

      if (filePath) {
        await invoke("save_config_to_path", { content: mikrotikConfigContent, filePath });
        onShowToast("MikroTik 配置文件已保存", "success");
      }
    } catch (err) {
      onShowToast("保存失败: " + err, "error");
    }
  };

  // 保存 OpenWrt 配置文件
  const handleSaveOpenwrtConfig = async () => {
    try {
      const filePath = await save({
        defaultPath: `openwrt_peer.sh`,
        filters: [{
          name: 'Shell 脚本',
          extensions: ['sh', 'txt']
        }]
      });

      if (filePath) {
        await invoke("save_config_to_path", { content: openwrtConfigContent, filePath });
        onShowToast("OpenWrt 配置文件已保存", "success");
      }
    } catch (err) {
      onShowToast("保存失败: " + err, "error");
    }
  };

  // 打开外部链接
  const handleOpenExternalLink = async (url) => {
    try {
      await openUrl(url);
    } catch (err) {
      console.error("打开链接失败:", err);
      onShowToast("打开链接失败: " + err, "error");
    }
  };

  return (
    <>
      {/* 标签页导航 */}
      <div className="tabs-nav">
        <button
          className={`tab-button ${activeTab === "wireguard" ? "active" : ""}`}
          onClick={() => onSetActiveTab("wireguard")}
        >
          WireGuard
        </button>
        <button
          className={`tab-button ${activeTab === "qrcode" ? "active" : ""}`}
          onClick={() => onSetActiveTab("qrcode")}
        >
          二维码
        </button>
        <button
          className={`tab-button ${activeTab === "surge" ? "active" : ""}`}
          onClick={() => onSetActiveTab("surge")}
        >
          Surge
        </button>
        <button
          className={`tab-button ${activeTab === "ikuai" ? "active" : ""}`}
          onClick={() => onSetActiveTab("ikuai")}
        >
          爱快
        </button>
        <button
          className={`tab-button ${activeTab === "mikrotik" ? "active" : ""}`}
          onClick={() => onSetActiveTab("mikrotik")}
        >
          MikroTik
        </button>
        <button
          className={`tab-button ${activeTab === "openwrt" ? "active" : ""}`}
          onClick={() => onSetActiveTab("openwrt")}
        >
          OpenWrt
        </button>
      </div>

      {/* 标签页内容 */}
      <div className="tabs-content">
        {/* 标准 WireGuard 配置 */}
        {activeTab === "wireguard" && (
          <div className="tab-panel">
            <div className="config-result">
              <div className="config-header">
                <h3>标准 WireGuard 配置（{interfaceName}.conf）</h3>
                <div className="button-group-inline">
                  <button onClick={() => handleCopyToClipboard(wgConfigContent, "WireGuard 配置")} className="btn-save">
                    📋 复制
                  </button>
                  <button onClick={handleSaveWgConfig} className="btn-save">
                    💾 另存为...
                  </button>
                </div>
              </div>
              <pre className="config-content">{wgConfigContent}</pre>
              <p className="hint">
                📱 用于手机、电脑等客户端，可直接导入 WireGuard 应用
              </p>
            </div>

            <div className="success-info">
              <h4>📋 使用说明</h4>
              <ol>
                <li>点击 <strong>"💾 另存为..."</strong> 保存为 <strong>{interfaceName}.conf</strong></li>
                <li>将配置文件导入到客户端设备，或使用二维码扫描导入</li>
                <li>客户端公钥: <code>{publicKey}</code></li>
              </ol>
            </div>
          </div>
        )}

        {/* 二维码 */}
        {activeTab === "qrcode" && (
          <div className="tab-panel">
            {qrcodeDataUrl ? (
              <div className="qrcode-container">
                <h4>扫码快速导入</h4>
                <img src={qrcodeDataUrl} alt="WireGuard 配置二维码" className="qrcode" />
                <p className="qrcode-hint">使用 WireGuard 客户端扫描二维码即可快速导入配置</p>
                <div className="hint-box" style={{ marginTop: "1rem" }}>
                  💡 支持 iOS、Android 等移动设备的 WireGuard 官方客户端
                </div>
              </div>
            ) : (
              <p className="hint">二维码生成失败，请使用配置文件导入</p>
            )}
          </div>
        )}

        {/* Surge 配置 */}
        {activeTab === "surge" && (
          <div className="tab-panel">
            <div className="config-result">
              <div className="config-header">
                <h3>Surge WireGuard 配置</h3>
                <div className="button-group-inline">
                  <button onClick={() => handleCopyToClipboard(surgeConfigContent, "Surge 配置")} className="btn-save">
                    📋 复制
                  </button>
                  <button onClick={handleSaveSurgeConfig} className="btn-save">
                    💾 另存为...
                  </button>
                </div>
              </div>
              <pre className="config-content">{surgeConfigContent}</pre>
            </div>

            <div className="info-row">
              <div className="success-info">
                <h4>📋 使用说明</h4>
                <ol>
                  <li>点击 <strong>"💾 另存为..."</strong> 保存配置文件</li>
                  <li>打开 Surge 应用，进入配置编辑模式</li>
                  <li>将配置内容复制粘贴到 Surge 配置文件中</li>
                  <li>在 <code>[Proxy Group]</code> 中引用: <code>wireguard-{interfaceName.replace(/\s+/g, '')}</code></li>
                </ol>
              </div>

              <div className="hint-box">
                <h4>💡 注意事项</h4>
                <p><strong>支持平台：</strong>iOS、macOS</p>
                <p><strong>参考文档：</strong><span onClick={() => handleOpenExternalLink("https://manual.nssurge.com/policy/wireguard.html")} style={{ color: "var(--primary-color)", marginLeft: "0.5rem", cursor: "pointer", textDecoration: "underline" }}>Surge WireGuard 官方文档</span></p>
              </div>
            </div>
          </div>
        )}

        {/* 爱快配置 */}
        {activeTab === "ikuai" && (
          <div className="tab-panel">
            <div className="config-result">
              <div className="config-header">
                <h3>爱快路由器 Peer 配置 {allPeerConfigs.length > 1 && ` - 已累积 ${allPeerConfigs.length} 条`}</h3>
                <button onClick={onSavePeerConfig} className="btn-save">
                  💾 另存为...
                </button>
              </div>
              <pre className="config-content">{allPeerConfigs.join('\n')}</pre>
              <p className="hint">
                {allPeerConfigs.length > 1 && `包含本次会话生成的所有 ${allPeerConfigs.length} 条配置`}
              </p>
            </div>

            <div className="success-info">
              <h4>📋 使用说明：</h4>
              <ol>
                <li>点击 <strong>"💾 另存为..."</strong> 按钮保存为 <strong>peer.txt</strong></li>
                <li><strong>爱快路由器</strong>：在管理界面 → 网络设置 → VPN → WireGuard → Peer 管理中导入</li>
                <li><strong>OpenWrt</strong>：请参考配置中的参数手动添加 Peer</li>
              </ol>
            </div>
          </div>
        )}

        {/* MikroTik 配置 */}
        {activeTab === "mikrotik" && (
          <div className="tab-panel">
            <div className="config-result">
              <div className="config-header">
                <h3>MikroTik RouterOS Peer 配置</h3>
                <div className="button-group-inline">
                  <button onClick={() => handleCopyToClipboard(mikrotikConfigContent, "MikroTik 配置")} className="btn-save">
                    📋 复制
                  </button>
                  <button onClick={handleSaveMikrotikConfig} className="btn-save">
                    💾 另存为...
                  </button>
                </div>
              </div>
              <pre className="config-content">{mikrotikConfigContent}</pre>
            </div>

            <div className="info-row">
              <div className="success-info">
                <h4>📋 使用说明</h4>
                <ol>
                  <li>复制上方生成的命令</li>
                  <li>登录到 MikroTik RouterOS 设备终端（SSH 或 Winbox）</li>
                  <li>粘贴并执行命令，即可添加 WireGuard Peer</li>
                  <li>确认 <code>interface</code> 参数与接口名称一致</li>
                </ol>
              </div>

              <div className="hint-box">
                <h4>💡 注意事项</h4>
                <p>• 确保 WireGuard 接口已在 RouterOS 中创建</p>
                <p>• <code>interface</code> 参数需与实际接口名称匹配</p>
                <p>• 执行命令前建议先备份当前配置</p>
                <p><strong>参考文档：</strong><span onClick={() => handleOpenExternalLink("https://help.mikrotik.com/docs/display/ROS/WireGuard")} style={{ color: "var(--primary-color)", marginLeft: "0.5rem", cursor: "pointer", textDecoration: "underline" }}>MikroTik 官方文档</span></p>
              </div>
            </div>
          </div>
        )}

        {/* OpenWrt 配置 */}
        {activeTab === "openwrt" && (
          <div className="tab-panel">
            <div className="config-result">
              <div className="config-header">
                <h3>OpenWrt UCI Peer 配置</h3>
                <div className="button-group-inline">
                  <button onClick={() => handleCopyToClipboard(openwrtConfigContent, "OpenWrt 配置")} className="btn-save">
                    📋 复制
                  </button>
                  <button onClick={handleSaveOpenwrtConfig} className="btn-save">
                    💾 另存为...
                  </button>
                </div>
              </div>
              <pre className="config-content">{openwrtConfigContent}</pre>
            </div>

            <div className="info-row">
              <div className="success-info">
                <h4>📋 使用说明</h4>
                <ol>
                  <li>复制上方生成的 UCI 命令</li>
                  <li>登录到 OpenWrt 设备的 SSH 终端</li>
                  <li>粘贴并执行命令，即可添加 WireGuard Peer</li>
                  <li>确认接口已创建（例如 <code>{interfaceName}</code>）</li>
                  <li>可通过 <code>uci show network | grep wireguard</code> 查看配置</li>
                </ol>
              </div>

              <div className="hint-box">
                <h4>💡 注意事项</h4>
                <p>• 确保已安装软件包：<code>luci-proto-wireguard</code></p>
                <p>• 命令会自动提交配置并重启接口</p>
                <p>• 执行前建议备份：<code>sysupgrade -b /tmp/backup.tar.gz</code></p>
                <p><strong>参考文档：</strong><span onClick={() => handleOpenExternalLink("https://openwrt.org/docs/guide-user/services/vpn/wireguard/basics")} style={{ color: "var(--primary-color)", marginLeft: "0.5rem", cursor: "pointer", textDecoration: "underline" }}>OpenWrt 官方文档</span></p>
              </div>
            </div>
          </div>
        )}
      </div>
    </>
  );
}

export default ConfigTabs;
