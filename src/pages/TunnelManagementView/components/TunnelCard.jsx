function TunnelCard({
  tunnel,
  loading,
  onStart,
  onStop,
  onEdit,
  onViewDetails,
  onViewPeerList,
  onDelete,
  formatBytes,
  formatTime,
}) {
  return (
    <div className="tunnel-card">
      <div className="tunnel-card-header">
        <div className="tunnel-info">
          <h3>{tunnel.name}</h3>
          <span className={`tunnel-status status-${tunnel.status}`}>
            {tunnel.status === 'running' ? '🟢 运行中' :
             tunnel.status === 'stopped' ? '🔴 已停止' :
             '🟡 连接中'}
          </span>
        </div>
        <div className="tunnel-actions-inline">
          {tunnel.status === 'running' ? (
            <button
              onClick={() => onStop(tunnel.id)}
              className="btn-danger"
              disabled={loading}
              title={loading ? '操作中...' : '停止隧道'}
            >
              停止
            </button>
          ) : (
            <button
              onClick={() => onStart(tunnel.id)}
              className="btn-success"
              disabled={loading}
              title={loading ? '操作中...' : '启动隧道'}
            >
              启动
            </button>
          )}
          <button
            onClick={() => onEdit(tunnel)}
            className="btn-secondary"
            disabled={loading || tunnel.status === 'running'}
            title={tunnel.status === 'running' ? '请先停止隧道' : '编辑配置'}
          >
            编辑
          </button>
          <button
            onClick={() => onViewDetails(tunnel.id)}
            className="btn-secondary"
          >
            详情
          </button>
          {tunnel.mode === 'server' && tunnel.peers && tunnel.peers.length > 0 && (
            <button
              onClick={() => onViewPeerList(tunnel.id)}
              className="btn-secondary"
            >
              Peer 列表
            </button>
          )}
          <button
            onClick={() => onDelete(tunnel.id)}
            className="btn-danger-outline"
            disabled={loading || tunnel.status === 'running'}
            title={tunnel.status === 'running' ? '请先停止隧道' : '删除配置'}
          >
            删除
          </button>
        </div>
      </div>

      <div className="tunnel-card-body">
        <div className="tunnel-stat">
          <span className="stat-label">本地地址:</span>
          <span className="stat-value">{tunnel.address || 'N/A'}</span>
        </div>
        <div className="tunnel-stat">
          <span className="stat-label">对端:</span>
          <span className="stat-value">{tunnel.endpoint || 'N/A'}</span>
        </div>
        {tunnel.status === 'running' && (
          <>
            <div className="tunnel-stat">
              <span className="stat-label">上传:</span>
              <span className="stat-value">{formatBytes(tunnel.tx_bytes || 0)}</span>
            </div>
            <div className="tunnel-stat">
              <span className="stat-label">下载:</span>
              <span className="stat-value">{formatBytes(tunnel.rx_bytes || 0)}</span>
            </div>
            <div className="tunnel-stat">
              <span className="stat-label">最后握手:</span>
              <span className="stat-value">{formatTime(tunnel.last_handshake)}</span>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export default TunnelCard;
