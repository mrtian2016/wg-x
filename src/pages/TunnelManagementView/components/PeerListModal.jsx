function PeerListModal({ tunnel, onClose, onViewPeerConfig }) {
  if (!tunnel) {
    return null;
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h3>Peer 列表 - {tunnel.name}</h3>
          <button onClick={onClose} className="btn-close">
            ✕
          </button>
        </div>
        <div className="modal-body">
          {tunnel.peers && tunnel.peers.length > 0 ? (
            <div className="peer-list-container">
              {tunnel.peers.map((peer, index) => (
                <div key={index} className="peer-list-item">
                  <div className="peer-list-item-header">
                    <h4>Peer {index + 1}</h4>
                    <button
                      onClick={() => onViewPeerConfig(index)}
                      className="btn-secondary"
                    >
                      查看配置
                    </button>
                  </div>
                  <div className="peer-list-item-body">
                    <div className="detail-group">
                      <label>公钥:</label>
                      <div className="code-block">{peer.public_key}</div>
                    </div>
                    <div className="detail-group">
                      <label>允许的 IP:</label>
                      <div>{peer.allowed_ips}</div>
                    </div>
                    {peer.preshared_key && (
                      <div className="detail-group">
                        <label>预共享密钥:</label>
                        <div className="code-block">{peer.preshared_key}</div>
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="empty-state">
              <p>暂无 Peer 配置</p>
            </div>
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

export default PeerListModal;
