import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';

/**
 * 监听后端推送的 peer 统计数据更新
 * @param {Function} onStatsUpdate - 回调函数，接收 peer 统计数据
 * @example
 * usePeerStatsListener((peerStats) => {
 *   // peerStats 是 { public_key: [tx_bytes, rx_bytes, last_handshake] }
 *   console.log(peerStats);
 * });
 */
export function usePeerStatsListener(onStatsUpdate) {
  useEffect(() => {
    let unlisten;

    // 监听后端推送的事件
    listen('peer-stats-updated', (event) => {
      const peerStats = event.payload;
      if (onStatsUpdate && typeof onStatsUpdate === 'function') {
        onStatsUpdate(peerStats);
      }
    }).then((fn) => {
      unlisten = fn;
    }).catch((error) => {
      console.error('监听 peer-stats-updated 事件失败:', error);
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, [onStatsUpdate]);
}
