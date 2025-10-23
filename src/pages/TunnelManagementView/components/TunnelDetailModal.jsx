function TunnelDetailModal({ tunnel, onClose, formatBytes, formatTime }) {
  if (!tunnel) {
    return null;
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h3>隧道详情</h3>
          <button onClick={onClose} className="btn-close">
            ✕
          </button>
        </div>
        <div className="modal-body">
          <div className="detail-group">
            <label>隧道名称:</label>
            <div>{tunnel.name}</div>
          </div>
          <div className="detail-group">
            <label>运行模式:</label>
            <div>
              <span className="mode-badge" data-mode={tunnel.mode || 'server'}>
                {tunnel.mode === 'server' ? '服务端' : tunnel.mode === 'client' ? '💻 客户端' : '服务端'}
              </span>
            </div>
          </div>
          <div className="detail-group">
            <label>状态:</label>
            <div>
              <span className={`tunnel-status status-${tunnel.status}`}>
                {tunnel.status === 'running' ? '🟢 运行中' : '🔴 已停止'}
              </span>
            </div>
          </div>
          <div className="detail-group">
            <label>本地地址:</label>
            <div>{tunnel.address}</div>
          </div>
          <div className="detail-group">
            <label>监听端口:</label>
            <div>{tunnel.listen_port || 'Auto'}</div>
          </div>
          <div className="detail-group">
            <label>对端地址:</label>
            <div>{tunnel.endpoint}</div>
          </div>
          <div className="detail-group">
            <label>AllowedIPs:</label>
            <div>{tunnel.allowed_ips}</div>
          </div>
          <div className="detail-group">
            <label>公钥:</label>
            <div className="code-block">{tunnel.public_key}</div>
          </div>
          {tunnel.status === 'running' && (
            <>
              <div className="detail-group">
                <label>上传流量:</label>
                <div>{formatBytes(tunnel.tx_bytes || 0)}</div>
              </div>
              <div className="detail-group">
                <label>下载流量:</label>
                <div>{formatBytes(tunnel.rx_bytes || 0)}</div>
              </div>
              <div className="detail-group">
                <label>最后握手:</label>
                <div>{formatTime(tunnel.last_handshake)}</div>
              </div>
            </>
          )}
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

export default TunnelDetailModal;
