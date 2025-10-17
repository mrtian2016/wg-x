import { useState, useEffect } from "react";
import "./style.css";

function ConfirmDialog({ isOpen, title, message, onConfirm, onCancel }) {
  const [show, setShow] = useState(false);

  useEffect(() => {
    if (isOpen) {
      // 延迟显示，触发动画
      setTimeout(() => setShow(true), 10);
    } else {
      setShow(false);
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleConfirm = () => {
    setShow(false);
    setTimeout(() => onConfirm(), 200);
  };

  const handleCancel = () => {
    setShow(false);
    setTimeout(() => onCancel(), 200);
  };

  return (
    <div className={`confirm-overlay ${show ? "show" : ""}`} onClick={handleCancel}>
      <div className={`confirm-dialog ${show ? "show" : ""}`} onClick={(e) => e.stopPropagation()}>
        <div className="confirm-header">
          <h3>{title}</h3>
        </div>
        <div className="confirm-body">
          <p>{message}</p>
        </div>
        <div className="confirm-footer">
          <button className="btn-cancel" onClick={handleCancel}>
            取消
          </button>
          <button className="btn-confirm" onClick={handleConfirm}>
            确定
          </button>
        </div>
      </div>
    </div>
  );
}

export default ConfirmDialog;
