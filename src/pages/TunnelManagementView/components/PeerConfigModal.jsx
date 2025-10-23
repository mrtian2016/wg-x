import { useState } from 'react';
import PeerQrcodeDisplay from './PeerQrcodeDisplay';

function PeerConfigModal({
  peerIndex,
  tunnel,
  onClose,
  generateDetailPeerConfig,
  generateSurgeDetailPeerConfig,
  generatePeerQrcode,
  handleCopyPeerConfig,
  handleSavePeerConfig,
}) {
  const [activePeerConfigTab, setActivePeerConfigTab] = useState('wireguard');

  if (peerIndex === null || !tunnel) {
    return null;
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h3>Peer {peerIndex + 1} 配置</h3>
          <button onClick={onClose} className="btn-close">
            ✕
          </button>
        </div>
        <div className="modal-body">
          {/* 配置类型 Tab */}
          <div className="config-type-tabs">
            <button
              className={`config-type-btn ${activePeerConfigTab === 'wireguard' ? 'active' : ''}`}
              onClick={() => setActivePeerConfigTab('wireguard')}
            >
              WireGuard
            </button>
            <button
              className={`config-type-btn ${activePeerConfigTab === 'qrcode' ? 'active' : ''}`}
              onClick={() => setActivePeerConfigTab('qrcode')}
            >
              二维码
            </button>
            <button
              className={`config-type-btn ${activePeerConfigTab === 'surge' ? 'active' : ''}`}
              onClick={() => setActivePeerConfigTab('surge')}
            >
              Surge
            </button>
          </div>

          {/* WireGuard 配置 */}
          {activePeerConfigTab === 'wireguard' && (
            <div className="config-result">
              <div className="config-header">
                <h5>WireGuard 配置</h5>
                <div className="button-group-inline">
                  <button
                    onClick={() => handleCopyPeerConfig(peerIndex, 'wireguard', tunnel)}
                    className="btn-save"
                  >
                    📋 复制
                  </button>
                  <button
                    onClick={() => handleSavePeerConfig(peerIndex, 'wireguard', tunnel)}
                    className="btn-save"
                  >
                    💾 下载
                  </button>
                </div>
              </div>
              <pre className="config-content">{generateDetailPeerConfig(peerIndex, tunnel)}</pre>
            </div>
          )}

          {/* 二维码 */}
          {activePeerConfigTab === 'qrcode' && (
            <PeerQrcodeDisplay
              peerIndex={peerIndex}
              generateQrcode={(index) => generatePeerQrcode(index, tunnel)}
            />
          )}

          {/* Surge 配置 */}
          {activePeerConfigTab === 'surge' && (
            <div className="config-result">
              <div className="config-header">
                <h5>Surge 配置</h5>
                <div className="button-group-inline">
                  <button
                    onClick={() => handleCopyPeerConfig(peerIndex, 'surge', tunnel)}
                    className="btn-save"
                  >
                    📋 复制
                  </button>
                  <button
                    onClick={() => handleSavePeerConfig(peerIndex, 'surge', tunnel)}
                    className="btn-save"
                  >
                    💾 下载
                  </button>
                </div>
              </div>
              <pre className="config-content">{generateSurgeDetailPeerConfig(peerIndex, tunnel)}</pre>
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

export default PeerConfigModal;
