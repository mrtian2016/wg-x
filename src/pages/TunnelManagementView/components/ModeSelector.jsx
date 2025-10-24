function ModeSelector({ onClose, onSelectMode }) {
  return (
    <div className="modal-overlay">
      <div className="modal-content" onClick={(e) => e.stopPropagation()} style={{ maxWidth: '500px' }}>
        <div className="modal-header">
          <h3>选择隧道运行模式</h3>
          <button onClick={onClose} className="btn-close">
            ✕
          </button>
        </div>
        <div className="modal-body">
          <p style={{ marginBottom: '2rem', color: '#666', fontSize: '0.95rem' }}>
            请选择隧道的运行模式。创建后无法修改，请谨慎选择。
          </p>
          <div className="mode-selector">
            <button
              onClick={() => onSelectMode('server')}
              className="mode-option-btn"
            >
              <div className="mode-option-icon">🖥️</div>
              <div className="mode-option-title">服务端模式</div>
              <div className="mode-option-desc">作为 VPN 服务端，管理多个客户端连接</div>
            </button>
            <button
              onClick={() => onSelectMode('client')}
              className="mode-option-btn"
            >
              <div className="mode-option-icon">💻</div>
              <div className="mode-option-title">客户端模式</div>
              <div className="mode-option-desc">作为 VPN 客户端，连接到一个服务端</div>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export default ModeSelector;
