import Icon from "./icons";

interface CurtainOverlayProps {
  title: string;
  onRestore: () => void;
  onHideWindow: () => void;
}

export default function CurtainOverlay({ title, onRestore, onHideWindow }: CurtainOverlayProps) {
  return (
    <div className="reader-curtain" role="dialog" aria-label="阅读内容已遮挡">
      <Icon name="book" size={26} />
      <strong>{title}</strong>
      <p>阅读内容已遮挡</p>
      <div className="curtain-actions">
        <button className="btn btn-primary" onClick={onRestore}>
          恢复阅读
          <Icon name="chevron-right" size={15} />
        </button>
        <button className="btn btn-secondary" onClick={onHideWindow}>
          隐藏窗口
        </button>
      </div>
    </div>
  );
}
