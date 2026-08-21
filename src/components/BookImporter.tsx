import { useCallback, useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { listen } from "@tauri-apps/api/event";
import { useLibraryStore } from "../store/library";
import BookLogo from "./BookLogo";

interface ImportProgress {
  stage: string;
  file: string;
}

interface ImportResult {
  file: string;
  success: boolean;
  error?: string;
}

export default function BookImporter() {
  const { importBook, error, clearError } = useLibraryStore();
  const [dragOver, setDragOver] = useState(false);
  const [importing, setImporting] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [importResults, setImportResults] = useState<ImportResult[] | null>(null);

  useEffect(() => {
    const webview = getCurrentWebview();
    const unlistenDrop = webview.onDragDropEvent(async (event) => {
      if (event.payload.type === "over") {
        setDragOver(true);
      } else if (event.payload.type === "leave") {
        setDragOver(false);
      } else if (event.payload.type === "drop") {
        setDragOver(false);
        const paths = event.payload.paths.filter((p) => {
          const lower = p.toLowerCase();
          return lower.endsWith(".epub") || lower.endsWith(".txt");
        });
        if (paths.length > 0) {
          await handleFiles(paths);
        } else {
          setImportError("请拖入 .epub 或 .txt 文件");
        }
      }
    });
    return () => {
      unlistenDrop.then((un) => un());
    };
  }, []);

  useEffect(() => {
    const unlistenProgress = listen<ImportProgress>("import-progress", (e) => {
      setProgress(e.payload);
    });
    const unlistenComplete = listen("import-complete", () => {
      setProgress(null);
    });
    const unlistenError = listen<string>("import-error", (e) => {
      setImportError(e.payload);
      setProgress(null);
    });
    return () => {
      unlistenProgress.then((un) => un());
      unlistenComplete.then((un) => un());
      unlistenError.then((un) => un());
    };
  }, []);

  const handleFiles = useCallback(
    async (files: string[]) => {
      setImporting(true);
      setImportError(null);
      setImportResults(null);
      const results: ImportResult[] = [];

      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        const lower = file.toLowerCase();
        if (!lower.endsWith(".epub") && !lower.endsWith(".txt")) continue;

        const fileName = file.split(/[\\/]/).pop() || file;
        setProgress({
          stage: `导入 ${i + 1}/${files.length}`,
          file: fileName,
        });

        try {
          const result = await importBook(file);
          if (result) {
            results.push({ file: fileName, success: true });
          } else {
            results.push({ file: fileName, success: false, error: "导入返回空结果" });
          }
        } catch (e) {
          results.push({ file: fileName, success: false, error: String(e) });
        }
      }

      setImporting(false);
      setProgress(null);

      const successCount = results.filter((r) => r.success).length;
      const failCount = results.filter((r) => !r.success).length;

      if (failCount > 0) {
        setImportResults(results);
        if (successCount > 0) {
          setImportError(`成功导入 ${successCount} 本，${failCount} 本失败`);
        } else {
          setImportError(`导入失败，请检查文件是否为有效的 EPUB 或 TXT`);
        }
      }
    },
    [importBook]
  );

  const handleOpenDialog = async () => {
    setImportError(null);
    setImportResults(null);
    try {
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "电子书",
            extensions: ["epub", "txt"],
          },
        ],
      });
      if (selected) {
        const files = Array.isArray(selected) ? selected : [selected];
        await handleFiles(files);
      }
    } catch (e) {
      setImportError(`打开对话框失败: ${e}`);
    }
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
  };

  const dismissError = () => {
    setImportError(null);
    setImportResults(null);
    clearError();
  };

  return (
    <div
      className={`drop-zone ${dragOver ? "drag-over" : ""}`}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
    >
      <div className="icon">
        {importing ? (
          <div style={{ fontSize: 64 }}>⏳</div>
        ) : (
          <BookLogo size={72} />
        )}
      </div>
      <div className="text">
        {progress
          ? progress.stage
          : importing
            ? "正在导入中，请稍后"
            : "拖拽 EPUB 或 TXT 文件到此处"}
      </div>
      {progress && (
        <div
          style={{
            fontSize: 12,
            color: "var(--text-muted)",
            marginTop: 4,
            maxWidth: 360,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {progress.file}
        </div>
      )}
      {!progress && (
        <div className="hint">
          或者{" "}
          <button
            onClick={handleOpenDialog}
            style={{
              color: "var(--accent)",
              textDecoration: "underline",
              background: "none",
              border: "none",
              cursor: "pointer",
              font: "inherit",
            }}
          >
            浏览文件
          </button>
        </div>
      )}

      {(importError || error) && (
        <div
          style={{
            marginTop: 12,
            padding: "8px 16px",
            background: "rgba(204, 51, 51, 0.1)",
            color: "#c33",
            borderRadius: 6,
            fontSize: 13,
            maxWidth: 480,
            textAlign: "center",
          }}
          onClick={dismissError}
        >
          <div>⚠ {importError || error}</div>
          {importResults && importResults.some((r) => !r.success) && (
            <div style={{ marginTop: 8, textAlign: "left", fontSize: 12 }}>
              {importResults
                .filter((r) => !r.success)
                .map((r, i) => (
                  <div key={i} style={{ opacity: 0.8 }}>
                    {r.file}: {r.error || "未知错误"}
                  </div>
                ))}
            </div>
          )}
          <span style={{ opacity: 0.6, fontSize: 11 }}>(点击关闭)</span>
        </div>
      )}
    </div>
  );
}
