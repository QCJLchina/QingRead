import { useReaderStore } from "../store/reader";

interface ChapterListProps {
  onSelect: (index: number) => void;
}

export default function ChapterList({ onSelect }: ChapterListProps) {
  const { toc, currentChapter, chapterCount } = useReaderStore();

  if (toc.length === 0) {
    // Generate from chapter count
    return (
      <ul className="chapter-list">
        {Array.from({ length: chapterCount }, (_, i) => (
          <li
            key={i}
            className={`chapter-item ${currentChapter?.index === i ? "active" : ""}`}
            onClick={() => onSelect(i)}
          >
            <span className="chapter-index">{i + 1}</span>
            <span className="chapter-title">第 {i + 1} 章</span>
          </li>
        ))}
      </ul>
    );
  }

  return (
    <ul className="chapter-list">
      {toc.map((entry, i) => {
        // 目录顺序不一定等于 spine 顺序，必须用后端映射出的章节索引跳转。
        const chapterIndex = entry.chapter_index ?? null;
        const disabled = chapterIndex === null;
        return (
          <li
            key={i}
            className={`chapter-item ${currentChapter?.index === chapterIndex ? "active" : ""} ${disabled ? "disabled" : ""}`}
            aria-disabled={disabled}
            onClick={() => {
              if (chapterIndex !== null) onSelect(chapterIndex);
            }}
            style={{
              paddingLeft: `${12 + entry.level * 16}px`,
              cursor: disabled ? "default" : "pointer",
            }}
          >
            <span className="chapter-index">{i + 1}</span>
            <span className="chapter-title">{entry.title}</span>
          </li>
        );
      })}
    </ul>
  );
}
