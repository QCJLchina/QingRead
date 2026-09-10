import Icon from "./icons";
import type { TocEntry } from "../types";

interface ReaderSidebarProps {
  bookTitle: string;
  chapterCount: number;
  toc: TocEntry[];
  currentIndex: number;
  overlay: boolean;
  onSelect: (chapterIndex: number) => void;
  onClose: () => void;
  onPrevChapter: () => void;
  onNextChapter: () => void;
}

export default function ReaderSidebar(props: ReaderSidebarProps) {
  const { bookTitle, chapterCount, toc, currentIndex, overlay, onSelect, onClose, onPrevChapter, onNextChapter } = props;

  const entries = toc.length > 0
    ? toc.map((entry, index) => ({
        key: "toc-" + index,
        label: entry.title,
        level: entry.level,
        index: entry.chapter_index,
      }))
    : Array.from({ length: chapterCount }, (_, index) => ({
        key: "chapter-" + index,
        label: "第 " + (index + 1) + " 章",
        level: 0,
        index,
      }));

  return (
    <aside className={"reader-sidebar" + (overlay ? " is-overlay" : "")} aria-label="章节目录">
      <div className="sidebar-header">
        <div>
          <div className="eyebrow">目录</div>
          <h3>{bookTitle || "本书"}</h3>
        </div>
        <button className="icon-button" onClick={onClose} aria-label="关闭目录">
          <Icon name="close" size={15} />
        </button>
      </div>

      <div className="sidebar-chapter-nav">
        <button onClick={onPrevChapter} disabled={currentIndex <= 0}>
          <Icon name="chevron-left" size={15} />
          上一章
        </button>
        <button onClick={onNextChapter} disabled={currentIndex >= chapterCount - 1}>
          下一章
          <Icon name="chevron-right" size={15} />
        </button>
      </div>

      <div className="sidebar-content">
        <ul className="chapter-list">
          {entries.map((entry) => {
            const disabled = entry.index === null;
            const active = entry.index === currentIndex;
            return (
              <li key={entry.key}>
                <button
                  className={"chapter-item" + (active ? " is-active" : "")}
                  aria-current={active ? "true" : undefined}
                  disabled={disabled}
                  onClick={() => {
                    if (entry.index !== null) onSelect(entry.index);
                  }}
                  style={{ paddingLeft: 10 + entry.level * 14 + "px" }}
                >
                  <span className="chapter-title">{entry.label}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </div>
    </aside>
  );
}
