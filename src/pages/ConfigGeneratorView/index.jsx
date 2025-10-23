import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import Stepper from "../../components/Stepper";
import ConfigTabs from "../../components/ConfigTabs";
import "./style.css";

export default function ConfigGeneratorView({ onShowToast, onNavPage }) {
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
  const [selectedServerId, setSelectedServerId] = useState("");
  const [selectedServerName, setSelectedServerName] = useState("");
  const [serverList, setServerList] = useState([]);

  // 标签页状态
  const [activeTab, setActiveTab] = useState("wireguard");

  // 初始化：加载配置
  useEffect(() => {
    const init = async () => {
      try {
        const dir = ".";
        setWorkDir(dir);

        // 加载环境变量
        const envConfig = await invoke("load_env_config", { workDir: dir });
        if (envConfig.interface_name) setInterfaceName(envConfig.interface_name);
        if (envConfig.listen_port) setListenPort(envConfig.listen_port);
        if (envConfig.dns_server) setDns(envConfig.dns_server);

        // 加载服务端列表
        const list = await invoke("get_server_list");
        setServerList(list);
      } catch (err) {
        console.error("初始化失败:", err);
      }
    };

    init();
  }, []);

  // 生成密钥对
  const handleGenerateKeypair = async () => {
    try {
      setLoading(true);
      const keypair = await invoke("generate_keypair");
      setPrivateKey(keypair.private_key);
      setPublicKey(keypair.public_key);
      onShowToast("密钥对已生成", "success");
    } catch (err) {
      onShowToast("生成密钥失败: " + err, "error");
    } finally {
      setLoading(false);
    }
  };

  // 生成预共享密钥
  const handleGeneratePSK = async () => {
    try {
      setLoading(true);
      const psk = await invoke("generate_preshared_key");
      setPresharedKey(psk);
      onShowToast("预共享密钥已生成", "success");
    } catch (err) {
      onShowToast("生成预共享密钥失败: " + err, "error");
    } finally {
      setLoading(false);
    }
  };

  // 从私钥计算公钥
  const handlePrivateKeyChange = async (value) => {
    setPrivateKey(value);
    if (value.length > 40) {
      try {
        const pub = await invoke("private_key_to_public", { privateKey: value });
        setPublicKey(pub);
      } catch (err) {
        setPublicKey("");
      }
    }
  };

  // 验证步骤
  const validateStep1 = () => {
    if (!selectedServerId) {
      onShowToast("请选择一个服务端", "warning");
      return false;
    }
    return true;
  };

  const validateStep2 = () => {
    if (!interfaceName.trim()) {
      onShowToast("请输入接口名称", "warning");
      return false;
    }
    if (interfaceName.includes(" ")) {
      onShowToast("接口名称不允许包含空格", "warning");
      return false;
    }

    if (!peerComment.trim()) {
      onShowToast("请输入客户端备注名称", "warning");
      return false;
    }
    if (peerComment.includes(" ")) {
      onShowToast("客户端备注名称不允许包含空格", "warning");
      return false;
    }

    if (!privateKey.trim()) {
      onShowToast("请生成或输入私钥", "warning");
      return false;
    }
    if (privateKey.includes(" ")) {
      onShowToast("私钥不允许包含空格", "warning");
      return false;
    }
    if (privateKey.length !== 44) {
      onShowToast("私钥长度必须为 44 个字符", "warning");
      return false;
    }

    if (!address.trim()) {
      onShowToast("请输入客户端 IP 地址", "warning");
      return false;
    }
    if (address.includes(" ")) {
      onShowToast("IP 地址不允许包含空格", "warning");
      return false;
    }
    const ipCidrRegex = /^(\d{1,3}\.){3}\d{1,3}\/\d{1,2}$/;
    if (!ipCidrRegex.test(address.trim())) {
      onShowToast("IP 地址格式错误，必须为 CIDR 格式（例如: 192.168.199.10/32）", "warning");
      return false;
    }

    const parts = address.trim().split('/');
    const ip = parts[0].split('.');
    const cidr = parseInt(parts[1]);

    for (let part of ip) {
      const num = parseInt(part);
      if (num < 0 || num > 255) {
        onShowToast("IP 地址每个部分必须在 0-255 之间", "warning");
        return false;
      }
    }

    if (cidr < 0 || cidr > 32) {
      onShowToast("CIDR 前缀长度必须在 0-32 之间", "warning");
      return false;
    }

    if (listenPort) {
      if (listenPort.includes(" ")) {
        onShowToast("监听端口不允许包含空格", "warning");
        return false;
      }
      const port = parseInt(listenPort);
      if (isNaN(port) || port < 1 || port > 65535) {
        onShowToast("监听端口必须在 1-65535 之间", "warning");
        return false;
      }
    }

    if (dns && dns.includes(" ")) {
      onShowToast("DNS 服务器不允许包含空格", "warning");
      return false;
    }

    return true;
  };

  // 下一步
  const handleNext = async () => {
    if (step === 1 && validateStep1()) {
      try {
        const server = await invoke("get_server_detail", { id: selectedServerId });
        setPeerPublicKey(server.peer_public_key);
        setPresharedKey(server.preshared_key);
        setEndpoint(server.endpoint);
        setAllowedIps(server.allowed_ips);
        setKeepalive(server.persistent_keepalive);
        setpeerInterface(server.peer_interface);

        const nextId = await invoke("get_next_peer_id_for_server", { serverId: selectedServerId });
        setpeerId(nextId);
      } catch (err) {
        onShowToast("加载服务端配置失败: " + err, "error");
        return;
      }
      setStep(2);
    } else if (step === 2 && validateStep2()) {
      await handleGenerate();
    }
  };

  // 上一步
  const handlePrev = () => {
    setStep(step - 1);
  };

  // 生成配置
  const handleGenerate = async () => {
    try {
      setLoading(true);
      onShowToast("正在生成配置...", "info");

      const config = {
        interface_name: interfaceName,
        private_key: privateKey,
        address: address,
        listen_port: listenPort || null,
        dns: dns || null,
        peer_public_key: peerPublicKey,
        preshared_key: presharedKey || null,
        endpoint: endpoint,
        allowed_ips: allowedIps,
        persistent_keepalive: keepalive || null,
        peer_id: peerId,
        peer_interface: peerInterface,
        peer_comment: peerComment,
      };

      const wgConfig = await invoke("generate_wg_config", { config, workDir });
      setWgConfigContent(wgConfig);

      const ikuaiConfig = await invoke("generate_ikuai_config", { config, workDir });

      const surgeConfig = await invoke("generate_surge_config", { config, workDir });
      setSurgeConfigContent(surgeConfig);

      const mikrotikConfig = await invoke("generate_mikrotik_config", { config, workDir });
      setMikrotikConfigContent(mikrotikConfig);

      const openwrtConfig = await invoke("generate_openwrt_config", { config, workDir });
      setOpenwrtConfigContent(openwrtConfig);

      setAllPeerConfigs(prev => [...prev, ikuaiConfig]);

      try {
        const qrcode = await invoke("generate_qrcode", { content: wgConfig });
        setQrcodeDataUrl(qrcode);
      } catch (err) {
        console.error("生成二维码失败:", err);
      }

      try {
        await invoke("update_server_peer_id", {
          serverId: selectedServerId,
          nextPeerId: peerId + 1
        });
      } catch (err) {
        console.error("更新 Peer ID 失败:", err);
      }

      try {
        const historyEntry = {
          id: Date.now().toString(),
          timestamp: Date.now(),
          interface_name: interfaceName,
          peer_comment: peerComment,
          peer_id: peerId,
          address: address,
          wg_config: wgConfig,
          ikuai_config: ikuaiConfig,
          surge_config: surgeConfig,
          mikrotik_config: mikrotikConfig,
          openwrt_config: openwrtConfig,
          public_key: publicKey,
          server_id: selectedServerId,
          server_name: selectedServerName,
        };
        await invoke("save_to_history", { entry: historyEntry });
      } catch (err) {
        console.error("保存历史记录失败:", err);
      }

      setStep(3);
      onShowToast("配置生成成功！", "success");
    } catch (err) {
      onShowToast("生成配置失败: " + err, "error");
    } finally {
      setLoading(false);
    }
  };

  // 保存 Peer 配置文件
  const handleSavePeerConfig = async () => {
    try {
      const filePath = await save({
        defaultPath: 'peers.txt',
        filters: [{
          name: 'Peer 配置',
          extensions: ['txt']
        }]
      });

      if (filePath) {
        const allContent = allPeerConfigs.join('\n');
        await invoke("save_config_to_path", { content: allContent, filePath });
        onShowToast(`已保存 ${allPeerConfigs.length} 条 Peer 配置`, "success");
      }
    } catch (err) {
      onShowToast("保存失败: " + err, "error");
    }
  };

  // 加载服务端列表
  const loadServerList = async () => {
    try {
      const list = await invoke("get_server_list");
      setServerList(list);
    } catch (err) {
      console.error("加载服务端列表失败:", err);
    }
  };

  // 重新开始
  const handleReset = async () => {
    setStep(1);

    setInterfaceName("wg0");
    setPrivateKey("");
    setPublicKey("");
    setAddress("");
    setListenPort("");
    setDns("");

    setpeerComment("");

    setWgConfigContent("");
    setSurgeConfigContent("");
    setQrcodeDataUrl("");

    setActiveTab("wireguard");

    if (selectedServerId) {
      try {
        const nextId = await invoke("get_next_peer_id_for_server", { serverId: selectedServerId });
        setpeerId(nextId);
      } catch (err) {
        console.error("获取下一个 Peer ID 失败:", err);
        setpeerId(1);
      }
    }
  };

  // 初始化时直接跳到第1步
  useEffect(() => {
    if (step === 0) {
      setStep(1);
    }
  }, []);

  return (
    <>

      {/* 主内容区域 */}
      <div className={step === 3 ? 'config-content-wrapper config-success' : 'config-content-wrapper '}>

        {/* Step 1: 选择服务端 */}
        {step === 1 && (
          <div className="form-section">

            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1rem" }}>
              <h2>选择 WireGuard 服务端</h2>
            </div>
            <div className="hint-box">
              💡 请选择要连接的 WireGuard 服务端，或点击"服务端管理"新建一个
            </div>

            {serverList.length === 0 ? (
              <div style={{ textAlign: "center", padding: "2rem" }}>
                <p className="hint">暂无服务端配置</p>
                <p className="hint">请先在"服务端管理"中添加服务端</p>
                <button
                  className="btn-primary"
                  style={{ marginTop: "1rem" }}
                  onClick={() => loadServerList()}
                >
                  刷新列表
                </button>
              </div>
            ) : (
              <>
                <div className="form-group">
                  <label>选择服务端 *</label>
                  <div className="custom-select">
                    <select
                      value={selectedServerId}
                      onChange={(e) => {
                        setSelectedServerId(e.target.value);
                        const server = serverList.find(s => s.id === e.target.value);
                        if (server) {
                          setSelectedServerName(server.name);
                        }
                      }}
                    >
                      <option value="">-- 请选择服务端 --</option>
                      {serverList.map(server => (
                        <option key={server.id} value={server.id}>
                          {server.name} ({server.endpoint})
                        </option>
                      ))}
                    </select>
                  </div>
                </div>

                {selectedServerId && (
                  <div style={{ background: "var(--bg-light)", padding: "1rem", borderRadius: "6px", marginTop: "1rem" }}>
                    <h4 style={{ margin: "0 0 0.75rem 0", fontSize: "1rem", fontWeight: "600" }}>服务端信息</h4>
                    {(() => {
                      const server = serverList.find(s => s.id === selectedServerId);
                      return server ? (
                        <div style={{ fontSize: "0.85rem", display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
                          <div><strong>名称:</strong> {server.name}</div>
                          <div><strong>Endpoint:</strong> {server.endpoint}</div>
                          <div><strong>接口:</strong> {server.peer_interface}</div>
                          <div><strong>Keepalive:</strong> {server.persistent_keepalive}s</div>
                          <div><strong>AllowedIPs:</strong> <code style={{ fontSize: "0.8rem" }}>{server.allowed_ips}</code></div>
                          <div><strong>下一个 ID:</strong> #{server.next_peer_id}</div>
                          {server.peer_address_range && (
                            <div><strong>Peer 范围:</strong> <code style={{ fontSize: "0.8rem" }}>{server.peer_address_range}</code></div>
                          )}
                          <div style={{ gridColumn: "1 / -1", marginTop: "0.25rem" }}><strong>公钥:</strong> <code style={{ fontSize: "0.75rem", wordBreak: "break-all" }}>{server.peer_public_key}</code></div>
                          {server.preshared_key && (
                            <div style={{ gridColumn: "1 / -1" }}><strong>PSK:</strong> <code style={{ fontSize: "0.75rem", wordBreak: "break-all" }}>{server.preshared_key}</code></div>
                          )}
                        </div>
                      ) : null;
                    })()}
                  </div>
                )}

                <div style={{ marginTop: "1rem", padding: "0.75rem", background: "#f8f9fa", borderRadius: "6px" }}>
                  <p style={{ margin: 0, fontSize: "0.9rem" }}>
                    需要添加或管理服务端？
                    <button
                      onClick={onNavPage}
                      className="btn-generate"
                      style={{ marginLeft: "0.5rem", fontSize: "0.85rem", padding: "0.3rem 0.6rem" }}
                    >
                      服务端管理
                    </button>
                  </p>
                </div>
              </>
            )}

            <div className="button-group" style={{ marginTop: "1.5rem" }}>
              <button onClick={handleNext} className="btn-primary" disabled={!selectedServerId}>
                下一步 →
              </button>
            </div>
          </div>
        )}


        {/* Step 2: 客户端配置 */}
        {step === 2 && (
          <div className="form-section">
             <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "1rem" }}>
                <h2>客户端配置</h2>
              </div>
               <div className="hint-box">
              💡 完成后将生成多平台配置：WireGuard 标准配置、Surge、爱快、MikroTik、OpenWrt
            </div>
            <div className="form-group">
              <label>客户端接口名称 *</label>
              <input
                type="text"
                value={interfaceName}
                onChange={(e) => setInterfaceName(e.target.value)}
                placeholder="wg0"
              />
            </div>
            <div className="form-group">
              <label>客户端备注名称 *</label>
              <input
                type="text"
                value={peerComment}
                onChange={(e) => setpeerComment(e.target.value)}
                placeholder="例如: iphone, macbook, laptop"
              />
              <small>用于识别设备的备注信息</small>
            </div>

            <div className="form-group">
              <label>本地私钥</label>
              <div className="key-input-group">
                <input
                  type="text"
                  value={privateKey}
                  onChange={(e) => handlePrivateKeyChange(e.target.value)}
                  placeholder="粘贴已有私钥或点击生成"
                />
                <button onClick={handleGenerateKeypair} disabled={loading} className="btn-generate">
                  {loading ? "生成中..." : "生成密钥对"}
                </button>
              </div>
            </div>

            {publicKey && (
              <div className="form-group">
                <label>本地公钥（提供给路由器服务端）</label>
                <input
                  type="text"
                  value={publicKey}
                  readOnly
                  className="readonly"
                />
              </div>
            )}

            <div className="form-group">
              <label>客户端 IP 地址 *</label>
              <div className="key-input-group">
                <input
                  type="text"
                  value={address}
                  onChange={(e) => setAddress(e.target.value)}
                  placeholder="例如: 192.168.199.10/32"
                />
                <button
                  onClick={async () => {
                    if (!selectedServerId) {
                      onShowToast("请先选择服务端", "warning");
                      return;
                    }
                    try {
                      const server = await invoke("get_server_detail", { id: selectedServerId });
                      if (!server.peer_address_range) {
                        onShowToast("服务端未配置 Peer 地址范围", "warning");
                        return;
                      }
                      setLoading(true);
                      const generatedIp = await invoke("generate_next_client_ip", {
                        peerAddressRange: server.peer_address_range,
                        serverId: selectedServerId
                      });
                      setAddress(generatedIp);
                      onShowToast("客户端 IP 已生成", "success");
                    } catch (err) {
                      onShowToast("生成 IP 地址 失败: " + err, "error");
                    } finally {
                      setLoading(false);
                    }
                  }}
                  disabled={loading}
                  className="btn-generate"
                >
                  生成 IP 地址
                </button>
              </div>
              <small>VPN 内网中分配给本设备的 IP 地址，必须使用 CIDR 格式（IP/前缀长度）</small>
            </div>
            <div className="form-row">
              <div className="form-group">
                <label>监听端口（可选）</label>
                <input
                  type="text"
                  value={listenPort}
                  onChange={(e) => setListenPort(e.target.value)}
                  placeholder="51820"
                />
              </div>

              <div className="form-group">
                <label>DNS 服务器（可选）</label>
                <input
                  type="text"
                  value={dns}
                  onChange={(e) => setDns(e.target.value)}
                  placeholder="8.8.8.8,1.1.1.1"
                />
              </div>
            </div>
            <div className="button-group">
              <button onClick={handlePrev} className="btn-secondary">
                上一步
              </button>
              <button onClick={handleNext} className="btn-primary" disabled={loading}>
                {loading ? "生成中..." : "生成配置 →"}
              </button>
            </div>
          </div>
        )}

        {/* Step 3: 配置结果 */}
        {step === 3 && (
          <div className="form-section">
            <h2>✅ 配置生成成功！</h2>


            <ConfigTabs
              activeTab={activeTab}
              onSetActiveTab={setActiveTab}
              interfaceName={interfaceName}
              wgConfigContent={wgConfigContent}
              qrcodeDataUrl={qrcodeDataUrl}
              surgeConfigContent={surgeConfigContent}
              allPeerConfigs={allPeerConfigs}
              mikrotikConfigContent={mikrotikConfigContent}
              openwrtConfigContent={openwrtConfigContent}
              publicKey={publicKey}
              onShowToast={onShowToast}
              onSavePeerConfig={handleSavePeerConfig}
            />
            <div className="button-group">
              {allPeerConfigs.length > 1 && (
                <button
                  onClick={() => {
                    setAllPeerConfigs([]);
                    onShowToast("已清空累积配置", "success");
                  }}
                  className="btn-secondary"
                >
                  清空累积配置
                </button>
              )}
              <button onClick={handleReset} className="btn-primary">
                生成下一个配置
              </button>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
