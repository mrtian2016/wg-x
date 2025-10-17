import { useState, useCallback } from 'react';

/**
 * Toast Hook - 用于管理全局消息通知
 * @returns {Object} { messages, showToast }
 */
export function useToast() {
  const [messages, setMessages] = useState([]);

  /**
   * 显示 Toast 消息
   * @param {string} message - 消息内容
   * @param {string} type - 消息类型: 'success' | 'error' | 'warning' | 'info'
   */
  const showToast = useCallback((message, type = 'success') => {
    const id = Date.now() + Math.random(); // 确保唯一 ID

    setMessages((prev) => {
      // 限制最多同时显示 5 条消息
      const newMessages = [...prev, { id, message, type }];
      return newMessages.slice(-5);
    });
  }, []);

  /**
   * 移除指定的 Toast 消息
   * @param {number} id - 消息 ID
   */
  const removeToast = useCallback((id) => {
    setMessages((prev) => prev.filter((msg) => msg.id !== id));
  }, []);

  /**
   * 根据消息内容自动判断类型
   * @param {string} message - 消息内容
   */
  const showAutoToast = useCallback((message) => {
    let type = 'success';
    if (message.includes('失败') || message.includes('错误')) {
      type = 'error';
    } else if (message.includes('警告')) {
      type = 'warning';
    }
    showToast(message, type);
  }, [showToast]);

  return {
    messages,
    showToast,
    removeToast,
    showAutoToast,
  };
}
