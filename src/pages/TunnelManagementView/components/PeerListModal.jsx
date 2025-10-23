function PeerListModal({ tunnel, onClose, onViewPeerConfig, formatBytes, formatTime }) {
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
                    <h4>
                      {peer.remark ? `${peer.remark}` : `Peer ${index + 1}`}
                      {peer.remark && <span style={{ fontSize: '0.9em', color: '#999', marginLeft: '8px' }}>({peer.address || 'N/A'})</span>}
                    </h4>
                    <button
                      onClick={() => onViewPeerConfig(index)}
                      className="btn-secondary"
                    >
                      查看配置
                    </button>
                  </div>
                  <div className="peer-list-item-body">
                    <div className="detail-group">
                      
                      <div><label>上传流量:</label>{formatBytes ? formatBytes(peer.tx_bytes || 0) : '0 B'}</div>
                    </div>
                    <div className="detail-group">
                      
                      <div><label>下载流量:</label>{formatBytes ? formatBytes(peer.rx_bytes || 0) : '0 B'}</div>
                    </div>
                    <div className="detail-group">
                      
                      <div><label>上次握手:</label>{formatTime ? formatTime(peer.last_handshake) : '从未'}</div>
                    </div>
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
