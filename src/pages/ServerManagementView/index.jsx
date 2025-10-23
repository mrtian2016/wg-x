import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ConfirmDialog from "../../components/ConfirmDialog";

function ServerManagementView({
  onShowToast,
}) {
  const [serverList, setServerList] = useState([]);
  const [selectedServer, setSelectedServer] = useState(null);
  const [showForm, setShowForm] = useState(false);
  const [isEditing, setIsEditing] = useState(false);

  // 确认对话框状态
  const [showConfirmDialog, setShowConfirmDialog] = useState(false);
  const [confirmAction, setConfirmAction] = useState(null);
  const [confirmTitle, setConfirmTitle] = useState("");
  const [confirmMessage, setConfirmMessage] = useState("");

  // 表单字段
  const [formData, setFormData] = useState({
    id: "",
    name: "",
    peer_public_key: "",
    preshared_key: "",
    endpoint: "",
    allowed_ips: "0.0.0.0/0,::/0",
    persistent_keepalive: "25",
    peer_interface: "wg_0",
    next_peer_id: 1,
    peer_address_range: "",
  });

  // 加载服务端列表
  const loadServerList = async () => {
    try {
      const list = await invoke("get_server_list");
      setServerList(list);
    } catch (err) {
      console.error("加载服务端列表失败:", err);
      onShowToast("加载服务端列表失败: " + err, "error");
    }
  };

  // 初始化加载
  useState(() => {
    loadServerList();
  }, []);

  // 查看服务端详情
  const handleViewServer = async (id) => {
    try {
      const detail = await invoke("get_server_detail", { id });
      setSelectedServer(detail);
    } catch (err) {
      onShowToast("加载服务端详情失败: " + err, "error");
    }
  };

  // 新建服务端
  const handleNewServer = () => {
    setFormData({
      id: Date.now().toString(),
      name: "",
      peer_public_key: "",
      preshared_key: "",
      endpoint: "",
      allowed_ips: "0.0.0.0/0,::/0",
      persistent_keepalive: "25",
      peer_interface: "wg_0",
      next_peer_id: 1,
      peer_address_range: "",
    });
    setIsEditing(false);
    setShowForm(true);
  };

  // 编辑服务端
  const handleEditServer = (server) => {
    setFormData({
      id: server.id,
      name: server.name,
      peer_public_key: server.peer_public_key,
      preshared_key: server.preshared_key,
      endpoint: server.endpoint,
      allowed_ips: server.allowed_ips,
      persistent_keepalive: server.persistent_keepalive,
      peer_interface: server.peer_interface,
      next_peer_id: server.next_peer_id,
      peer_address_range: server.peer_address_range || "",
    });
    setIsEditing(true);
    setShowForm(true);
    setSelectedServer(null);
  };

  // 保存服务端
  const handleSaveServer = async () => {
    // 验证必填项
    if (!formData.name.trim()) {
      onShowToast("请输入服务端名称", "warning");
      return;
    }

    // 验证服务端名称不包含空格
    if (formData.name.includes(" ")) {
      onShowToast("服务端名称不允许包含空格", "warning");
      return;
    }

    // 验证服务端公钥
    if (!formData.peer_public_key.trim()) {
      onShowToast("请输入服务端公钥", "warning");
      return;
    }
    if (formData.peer_public_key.includes(" ")) {
      onShowToast("服务端公钥不允许包含空格", "warning");
      return;
    }
    if (formData.peer_public_key.length !== 44) {
      onShowToast("服务端公钥长度必须为 44 个字符", "warning");
      return;
    }

    // 验证预共享密钥（如果提供了）
    if (formData.preshared_key) {
      if (formData.preshared_key.includes(" ")) {
        onShowToast("预共享密钥不允许包含空格", "warning");
        return;
      }
      if (formData.preshared_key.length !== 44) {
        onShowToast("预共享密钥长度必须为 44 个字符", "warning");
        return;
      }
    }

    // 验证 Endpoint 地址
    if (!formData.endpoint.trim()) {
      onShowToast("请输入 Endpoint 地址", "warning");
      return;
    }
    if (formData.endpoint.includes(" ")) {
      onShowToast("Endpoint 地址不允许包含空格", "warning");
      return;
    }
    // 验证 Endpoint 格式: IP:端口 或 域名:端口
    const endpointRegex = /^([a-zA-Z0-9.-]+):(\d+)$/;
    if (!endpointRegex.test(formData.endpoint)) {
      onShowToast("Endpoint 格式不正确，应为 IP:端口 或 域名:端口（例如: example.com:51820 或 1.2.3.4:51820）", "warning");
      return;
    }

    // 验证 AllowedIPs 格式（逗号分隔的 CIDR）
    if (!formData.allowed_ips.trim()) {
      onShowToast("请输入 AllowedIPs", "warning");
      return;
    }
    if (formData.allowed_ips.includes(" ")) {
      onShowToast("AllowedIPs 不允许包含空格", "warning");
      return;
    }
    // 移除所有空格后验证
    const allowedIpsClean = formData.allowed_ips.replace(/\s/g, "");
    const cidrList = allowedIpsClean.split(",").filter(ip => ip.length > 0);
    if (cidrList.length === 0) {
      onShowToast("AllowedIPs 不能为空", "warning");
      return;
    }
    // 验证每个 CIDR 格式 (IPv4/prefix 或 IPv6/prefix)
    const cidrRegex = /^([0-9]{1,3}\.){3}[0-9]{1,3}\/[0-9]{1,2}$|^([0-9a-fA-F:]+)\/[0-9]{1,3}$/;
    for (const cidr of cidrList) {
      if (!cidrRegex.test(cidr)) {
        onShowToast(`AllowedIPs 格式不正确: "${cidr}" 不是有效的 CIDR 格式（应为 IP/掩码，例如: 0.0.0.0/0 或 192.168.1.0/24）`, "warning");
        return;
      }
    }

    // 验证 PersistentKeepalive 不包含空格且为数字
    if (formData.persistent_keepalive.includes(" ")) {
      onShowToast("PersistentKeepalive 不允许包含空格", "warning");
      return;
    }
    if (formData.persistent_keepalive && isNaN(formData.persistent_keepalive)) {
      onShowToast("PersistentKeepalive 必须为数字", "warning");
      return;
    }

    // 验证接口名称不包含空格
    if (formData.peer_interface.includes(" ")) {
      onShowToast("接口名称不允许包含空格", "warning");
      return;
    }

    // 验证 Peer 地址范围（如果提供了）
    if (formData.peer_address_range.trim()) {
      if (formData.peer_address_range.includes(" ")) {
        onShowToast("Peer 地址范围不允许包含空格", "warning");
        return;
      }
      // CIDR 格式验证: IP/掩码
      const peerAddrRegex = /^([0-9]{1,3}\.){3}[0-9]{1,3}\/([0-9]{1,2})$/;
      if (!peerAddrRegex.test(formData.peer_address_range)) {
        onShowToast("Peer 地址范围格式不正确，应为 IP/掩码（例如: 10.2.3.0/24）", "warning");
        return;
      }
      // 检查掩码是否为 24
      const maskMatch = formData.peer_address_range.match(/\/([0-9]{1,2})$/);
      if (maskMatch && parseInt(maskMatch[1]) !== 24) {
        onShowToast("Peer 地址范围的掩码必须是 24", "warning");
        return;
      }
    }

    try {
      const serverConfig = {
        ...formData,
        created_at: isEditing
          ? (serverList.find(s => s.id === formData.id)?.created_at || Date.now())
          : Date.now(),
      };

      await invoke("save_server_config", { config: serverConfig });
      onShowToast(isEditing ? "服务端已更新" : "服务端已创建");

      setShowForm(false);
      setFormData({
        id: "",
        name: "",
        peer_public_key: "",
        preshared_key: "",
        endpoint: "",
        allowed_ips: "0.0.0.0/0,::/0",
        persistent_keepalive: "25",
        peer_interface: "wg_0",
        next_peer_id: 1,
        peer_address_range: "",
      });

      await loadServerList();
    } catch (err) {
      onShowToast("保存服务端失败: " + err, "error");
    }
  };

  // 删除服务端
  const handleDeleteServer = (id, name) => {
    setConfirmTitle("⚠️ 删除服务端");
    setConfirmMessage(`确定要删除服务端 "${name}" 吗？\n\n注意：删除后，关联的历史记录将无法正常显示服务端信息。`);
    setConfirmAction(() => async () => {
      try {
        await invoke("delete_server", { id });
        onShowToast("服务端已删除", "success");

        if (selectedServer && selectedServer.id === id) {
          setSelectedServer(null);
        }

        await loadServerList();
      } catch (err) {
        onShowToast("删除服务端失败: " + err, "error");
      }
    });
    setShowConfirmDialog(true);
  };

  // 生成预共享密钥
  const handleGeneratePSK = async () => {
    try {
      const psk = await invoke("generate_preshared_key");
      setFormData({ ...formData, preshared_key: psk });
      onShowToast("预共享密钥已生成", "success");
    } catch (err) {
      onShowToast("生成预共享密钥失败: " + err, "error");
    }
  };

  // 清空所有服务端配置
  const handleClearAllServers = () => {
    setConfirmTitle("⚠️ 清空所有服务端配置");
    setConfirmMessage(`确定要清空所有服务端配置吗？\n\n这会删除所有 ${serverList.length} 个服务端配置！\n\n注意：历史记录不会被删除，但历史记录中的服务端信息将无法显示。\n\n此操作不可恢复！`);
    setConfirmAction(() => async () => {
      try {
        await invoke("clear_all_servers");
        onShowToast("所有服务端配置已清空", "success");
        setServerList([]);
        setSelectedServer(null);
        setShowForm(false);
      } catch (err) {
        onShowToast("清空服务端配置失败: " + err, "error");
      }
    });
    setShowConfirmDialog(true);
  };

  return (
    <div className="form-section">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1rem" }}>
        <h2>服务端管理</h2>
      </div>

      {/* 表单界面 */}
      {showForm ? (
        <div>
          <h3 style={{marginBottom:'1rem'}}>{isEditing ? "编辑服务端" : "新建服务端"}</h3>

          <div className="form-group">
            <label>服务端名称 *</label>
            <input
              type="text"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              placeholder="例如: 家里路由器、办公室、云服务器"
            />
            <small>用于识别不同的 WireGuard 服务端</small>
          </div>

          <div className="form-group">
            <label>服务端公钥 *</label>
            <input
              type="text"
              value={formData.peer_public_key}
              onChange={(e) => setFormData({ ...formData, peer_public_key: e.target.value })}
              placeholder="从路由器管理界面获取"
            />
          </div>

          <div className="form-group">
            <label>预共享密钥（可选）</label>
            <div className="key-input-group">
              <input
                type="text"
                value={formData.preshared_key}
                onChange={(e) => setFormData({ ...formData, preshared_key: e.target.value })}
                placeholder="留空或点击生成"
              />
              <button onClick={handleGeneratePSK} className="btn-generate">
                生成 PSK
              </button>
            </div>
          </div>
          <div className="form-row">
          <div className="form-group">
            <label>Endpoint 地址 *</label>
            <input
              type="text"
              value={formData.endpoint}
              onChange={(e) => setFormData({ ...formData, endpoint: e.target.value })}
              placeholder="example.com:51820 或 1.2.3.4:51820"
            />
            <small>服务端的公网 IP 或域名 + 端口</small>
          </div>


          <div className="form-group">
            <label>AllowedIPs *</label>
            <input
              type="text"
              value={formData.allowed_ips}
              onChange={(e) => setFormData({ ...formData, allowed_ips: e.target.value })}
              placeholder="0.0.0.0/0,::/0"
            />
            <small>0.0.0.0/0 = 全局 VPN | 192.168.1.0/24 = 仅局域网流量</small>
          </div>
          </div>

          <div className="form-row">
            <div className="form-group">
              <label>PersistentKeepalive（秒）</label>
              <input
                type="text"
                value={formData.persistent_keepalive}
                onChange={(e) => setFormData({ ...formData, persistent_keepalive: e.target.value })}
                placeholder="25"
              />
              <small>推荐 25 秒，用于保持连接活跃</small>
            </div>

            <div className="form-group">
              <label>接口名称</label>
              <input
                type="text"
                value={formData.peer_interface}
                onChange={(e) => setFormData({ ...formData, peer_interface: e.target.value })}
                placeholder="wg_0"
              />
            </div>
          </div>

          <div className="form-group">
            <label>Peer 地址范围（可选）</label>
            <input
              type="text"
              value={formData.peer_address_range}
              onChange={(e) => setFormData({ ...formData, peer_address_range: e.target.value })}
              placeholder="例如: 10.2.3.0/24"
            />
            <small>WireGuard Peer 的地址池，CIDR 格式，掩码必须是 24（例如: 10.0.0.0/24）</small>
          </div>

          <div className="button-group">
            <button
              onClick={() => {
                setShowForm(false);
                setFormData({
                  id: "",
                  name: "",
                  peer_public_key: "",
                  preshared_key: "",
                  endpoint: "",
                  allowed_ips: "0.0.0.0/0,::/0",
                  persistent_keepalive: "25",
                  peer_interface: "wg_0",
                  next_peer_id: 1,
                  peer_address_range: "",
                });
              }}
              className="btn-secondary"
            >
              取消
            </button>
            <button onClick={handleSaveServer} className="btn-primary">
              保存
            </button>
          </div>
        </div>
      ) : (
        <>
          {/* 列表界面 */}
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "1rem", alignItems: "center", flexWrap: "wrap", gap: "0.5rem" }}>
            <p className="hint">共 {serverList.length} 个服务端</p>
            <div style={{ display: "flex", gap: "0.5rem" }}>
              {serverList.length > 0 && (
                <button onClick={handleClearAllServers} className="btn-secondary" style={{ fontSize: "0.8rem", padding: "0.4rem 0.7rem" }}>
                   清空所有服务端
                </button>
              )}
              <button onClick={handleNewServer} className="btn-primary" style={{ fontSize: "0.9rem" }}>
                新建服务端
              </button>
            </div>
          </div>

          {serverList.length === 0 ? (
            <p className="hint" style={{ textAlign: "center", padding: "2rem" }}>
              暂无服务端配置，点击"新建服务端"开始添加
            </p>
          ) : (
            <>
              <div style={{ display: "grid", gap: "0.5rem" }}>
                {serverList.map((server) => (
                  <div
                    key={server.id}
                    style={{
                      border: "1px solid var(--border-color)",
                      borderRadius: "6px",
                      padding: "0.75rem",
                      background: selectedServer?.id === server.id ? "var(--bg-light)" : "white",
                      cursor: "pointer",
                    }}
                    onClick={() => handleViewServer(server.id)}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                      <div>
                        <strong style={{ fontSize: "1rem" }}>{server.name}</strong>
                        <div style={{ fontSize: "0.85rem", color: "var(--text-muted)", marginTop: "0.25rem" }}>
                          {server.endpoint} | Peer ID 计数: {server.next_peer_id}
                        </div>
                      </div>
                      <div style={{ display: "flex", gap: "0.5rem" }}>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handleEditServer(server);
                          }}
                          className="btn-generate"
                          style={{ fontSize: "0.75rem", padding: "0.2rem 0.5rem" }}
                        >
                          编辑
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handleDeleteServer(server.id, server.name);
                          }}
                          className="btn-secondary"
                          style={{ fontSize: "0.75rem", padding: "0.2rem 0.5rem" }}
                        >
                          删除
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>

              {/* 详情显示 */}
              {selectedServer && (
                <div style={{ marginTop: "1rem", background: "var(--bg-light)", padding: "1rem", borderRadius: "6px" }}>
                  <h4 style={{ margin: "0 0 0.75rem 0", fontSize: "1rem", fontWeight: "600" }}>{selectedServer.name}</h4>
                  <div style={{ fontSize: "0.85rem", display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
                    <div><strong>Endpoint:</strong> {selectedServer.endpoint}</div>
                    <div><strong>接口:</strong> {selectedServer.peer_interface}</div>
                    <div><strong>Keepalive:</strong> {selectedServer.persistent_keepalive}s</div>
                    <div><strong>下一个 ID:</strong> #{selectedServer.next_peer_id}</div>
                    <div><strong>AllowedIPs:</strong> <code style={{ fontSize: "0.8rem" }}>{selectedServer.allowed_ips}</code></div>
                    <div><strong>创建时间:</strong> {new Date(selectedServer.created_at).toLocaleString()}</div>
                    {selectedServer.peer_address_range && (
                      <div><strong>Peer 范围:</strong> <code style={{ fontSize: "0.8rem" }}>{selectedServer.peer_address_range}</code></div>
                    )}
                    <div style={{ gridColumn: "1 / -1", marginTop: "0.25rem" }}><strong>公钥:</strong> <code style={{ fontSize: "0.75rem", wordBreak: "break-all" }}>{selectedServer.peer_public_key}</code></div>
                    {selectedServer.preshared_key && (
                      <div style={{ gridColumn: "1 / -1" }}><strong>PSK:</strong> <code style={{ fontSize: "0.75rem", wordBreak: "break-all" }}>{selectedServer.preshared_key}</code></div>
                    )}
                  </div>
                  <div className="button-group" style={{ marginTop: "1rem" }}>
                    <button onClick={() => handleEditServer(selectedServer)} className="btn-primary">
                      编辑此服务端
                    </button>
                  </div>
                </div>
              )}
            </>
          )}
        </>
      )}

      {/* 确认对话框 */}
      <ConfirmDialog
        isOpen={showConfirmDialog}
        title={confirmTitle}
        message={confirmMessage}
        onConfirm={() => {
          setShowConfirmDialog(false);
          if (confirmAction) {
            confirmAction();
          }
        }}
        onCancel={() => setShowConfirmDialog(false)}
      />
    </div>
  );
}

export default ServerManagementView;
