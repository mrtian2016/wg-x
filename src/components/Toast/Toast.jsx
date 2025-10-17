import { useEffect, useRef } from 'react';
import './style.css';

/**
 * Toast 消息通知组件
 * @param {Object} props
 * @param {Array} props.messages - 消息队列数组，每项包含 {id, message, type}
 * @param {Function} props.onRemove - 移除消息的回调函数
 */
function Toast({ messages, onRemove }) {
  const timersRef = useRef(new Map());

  useEffect(() => {
    // 为新消息设置自动清除定时器
    messages.forEach(({ id }) => {
      if (!timersRef.current.has(id)) {
        const timer = setTimeout(() => {
          onRemove(id);
          timersRef.current.delete(id);
        }, 2000);
        timersRef.current.set(id, timer);
      }
    });

    // 清理已不在消息列表中的定时器
    const currentMessageIds = new Set(messages.map(m => m.id));
    timersRef.current.forEach((timer, id) => {
      if (!currentMessageIds.has(id)) {
        clearTimeout(timer);
        timersRef.current.delete(id);
      }
    });

    // 组件卸载时清理所有定时器
    return () => {
      if (messages.length === 0) {
        timersRef.current.forEach((timer) => {
          clearTimeout(timer);
        });
        timersRef.current.clear();
      }
    };
  }, [messages, onRemove]);

  if (messages.length === 0) {
    return null;
  }

  return (
    <div className="wg-toast-container">
      {messages.map(({ id, message, type }) => (
        <div
          key={id}
          className={`wg-toast wg-toast-${type}`}
        >
          <div className="wg-toast-content">
            <span className="wg-toast-message">{message}</span>
          </div>
        </div>
      ))}
    </div>
  );
}

export default Toast;
