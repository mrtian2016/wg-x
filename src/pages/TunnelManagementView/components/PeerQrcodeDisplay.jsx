import { useState, useEffect } from 'react';

// Peer 二维码显示组件
function PeerQrcodeDisplay({ peerIndex, generateQrcode }) {
  const [qrcodeUrl, setQrcodeUrl] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    const loadQrcode = async () => {
      setLoading(true);
      setError(null);
      try {
        const url = await generateQrcode(peerIndex);
        if (url) {
          setQrcodeUrl(url);
        } else {
          setError('二维码生成失败');
        }
      } catch (err) {
        console.error('加载二维码出错:', err);
        setError('加载二维码出错: ' + err);
      } finally {
        setLoading(false);
      }
    };

    if (peerIndex !== undefined && generateQrcode) {
      loadQrcode();
    }
  }, [peerIndex, generateQrcode]);

  if (loading) {
    return (
      <div className="config-result">
        <div className="config-content" style={{ textAlign: 'center', padding: '2rem' }}>
          生成二维码中...
        </div>
      </div>
    );
  }

  if (error || !qrcodeUrl) {
    return (
      <div className="config-result">
        <div className="config-content" style={{ textAlign: 'center', padding: '2rem', color: '#d32f2f' }}>
          {error || '二维码生成失败，请使用其他配置方式'}
        </div>
      </div>
    );
  }

  return (
    <div className="config-result">
      <div className="qrcode-container">
        <h4>扫码快速导入</h4>
        <img src={qrcodeUrl} alt="WireGuard 配置二维码" className="qrcode" />
        <p className="qrcode-hint">使用 WireGuard 客户端扫描二维码即可快速导入配置</p>
        <div className="hint-box" style={{ marginTop: '1rem' }}>
          💡 支持 iOS、Android 等移动设备的 WireGuard 官方客户端
        </div>
      </div>
    </div>
  );
}

export default PeerQrcodeDisplay;
