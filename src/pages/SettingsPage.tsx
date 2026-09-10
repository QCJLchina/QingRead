import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { appApi } from "../api";
import { useSettingsStore } from "../store/settings";
import { useSyncStore, type SyncProgressPayload } from "../store/sync";
import type { SyncSummary, CloseBehavior } from "../types";


const kindLabels: Record<string, string> = {
  upload: "上传",
  download: "下载",
  delete: "删除",
  conflict: "冲突",
};

const fieldStyle = {
  width: "100%",
  padding: "10px 12px",
  borderRadius: 6,
  border: "1px solid var(--border-color)",
  background: "var(--card-bg)",
  color: "var(--text-primary)",
  fontSize: 14,
  boxSizing: "border-box" as const,
};

function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function formatTime(timestamp: number): string {
  if (!timestamp) return "-";
  return new Date(timestamp * 1000).toLocaleString();
}

export default function SettingsPage() {
  const [version, setVersion] = useState("");

  const { settings, updateSettings, setDataDir, restartApp } = useSettingsStore();
  const {
    serverUrl,
    username,
    passwordDraft,
    remoteDir,
    hasPassword,
    testing,
    previewing,
    applying,
    preview,
    decisions,
    progress,
    message,
    error,
    loadConfig,
    setField,
    saveConfig,
    clearConfig,
    testConnection,
    previewSync,
    setDecision,
    applySync,
    clearError,
  } = useSyncStore();

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  useEffect(() => {
    appApi.version().then(setVersion).catch(() => undefined);
  }, []);

  useEffect(() => {
    const unlistenProgress = listen<SyncProgressPayload>("sync-progress", (event) => {
      useSyncStore.setState({ progress: event.payload });
    });
    const unlistenComplete = listen<SyncSummary>("sync-complete", (event) => {
      useSyncStore.setState({
        progress: null,
        applying: false,
        message: event.payload.message,
      });
    });
    const unlistenError = listen<string>("sync-error", (event) => {
      useSyncStore.setState({ progress: null, applying: false, error: event.payload });
    });
    return () => {
      unlistenProgress.then((unlisten) => unlisten());
      unlistenComplete.then((unlisten) => unlisten());
      unlistenError.then((unlisten) => unlisten());
    };
  }, []);

  const conflicts = preview ? preview.actions.filter((action) => action.kind === "conflict") : [];
  const unresolvedConflicts = conflicts.filter((action) => !decisions[action.id]).length;

  const handleClearSync = async () => {
    if (confirm("确定清除 WebDAV 同步配置？")) {
      await clearConfig();
    }
  };

  const handleBgImageUpload = async () => {
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: "Images",
          extensions: ["jpg", "jpeg", "png", "webp", "gif"],
        },
      ],
    });
    if (selected) {
      const path = Array.isArray(selected) ? selected[0] : selected;
      updateSettings({ custom_bg_image: path });
    }
  };

  const handleSelectDataDir = async () => {
    const selected = await open({
      multiple: false,
      directory: true,
      defaultPath: settings.data_dir || undefined,
    });
    if (selected) {
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (confirm(`确定切换数据目录到:\n${path}\n\n切换后需要重启应用才能生效。`)) {
        await setDataDir(path);
        if (confirm("是否立即重启应用？")) {
          await restartApp();
        }
      }
    }
  };

  const handleResetDataDir = async () => {
    if (confirm("确定恢复为默认数据目录？\n\n需要重启应用才能生效。")) {
      await setDataDir("");
      if (confirm("是否立即重启应用？")) {
        await restartApp();
      }
    }
  };

  const handleRestartApp = async () => {
    if (confirm("确定重启应用？")) {
      await restartApp();
    }
  };

  const handleCloseBehaviorChange = (value: CloseBehavior) => {
    if (settings.close_behavior === value) return;
    updateSettings({ close_behavior: value });
    // 无需重启 — 后端实时更新托盘可见性
  };

  return (
    <div className="library-page" style={{ maxWidth: 800 }}>
      <div className="library-header">
        <h1>设置</h1>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 32 }}>
        {/* Custom Background */}
        <section>
          <h2 style={{ fontSize: 18, marginBottom: 16 }}>自定义背景</h2>
          <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
            <button onClick={handleBgImageUpload} className="btn btn-secondary">
              上传图片
            </button>
            {settings.custom_bg_image && (
              <button
                onClick={() => updateSettings({ custom_bg_image: null })}
                className="btn btn-secondary"
                style={{ color: "#c33" }}
              >
                移除
              </button>
            )}
          </div>
          {settings.custom_bg_image && (
            <div style={{ marginTop: 12, fontSize: 13, color: "var(--text-muted)" }}>
              当前: {settings.custom_bg_image}
            </div>
          )}
        </section>

        {/* Close Behavior */}
        <section>
          <h2 style={{ fontSize: 18, marginBottom: 16 }}>关闭行为</h2>
          <div style={{ fontSize: 13, color: "var(--text-secondary)", marginBottom: 12 }}>
            点击窗口右上角「×」按钮时的行为
          </div>
          <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
            {([
              { value: "quit" as const, label: "直接退出", desc: "完全关闭程序" },
              { value: "minimize_to_tray" as const, label: "最小化到托盘", desc: "保留托盘图标，可从托盘恢复" },
            ]).map((opt) => (
              <button
                key={opt.value}
                onClick={() => handleCloseBehaviorChange(opt.value)}
                style={{
                  padding: "12px 20px",
                  borderRadius: 6,
                  border: `2px solid ${settings.close_behavior === opt.value ? "var(--accent)" : "var(--border-color)"}`,
                  background: settings.close_behavior === opt.value ? "var(--accent)" : "var(--card-bg)",
                  color: settings.close_behavior === opt.value ? "white" : "var(--text-primary)",
                  textAlign: "left",
                  cursor: "pointer",
                  transition: "all 0.2s",
                  minWidth: 200,
                }}
              >
                <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 4 }}>{opt.label}</div>
                <div style={{ fontSize: 12, opacity: 0.8 }}>{opt.desc}</div>
              </button>
            ))}
          </div>
          <div style={{ marginTop: 12, fontSize: 12, color: "var(--text-muted)" }}>
            默认「直接退出」。选择「最小化到托盘」后托盘区域会出现图标，关闭主窗口时程序继续运行。
          </div>
        </section>

        {/* Data Directory */}
        <section>
          <h2 style={{ fontSize: 18, marginBottom: 16 }}>数据目录</h2>
          <div style={{ fontSize: 13, color: "var(--text-secondary)", marginBottom: 12, wordBreak: "break-all" }}>
            当前: {settings.data_dir || "默认（程序内嵌目录或系统标准目录）"}
          </div>
          <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
            <button onClick={handleSelectDataDir} className="btn btn-secondary">
              更改目录
            </button>
            <button
              onClick={handleResetDataDir}
              className="btn btn-secondary"
              disabled={!settings.data_dir}
            >
              恢复默认
            </button>
            <button
              onClick={handleRestartApp}
              className="btn btn-primary"
            >
              重启应用
            </button>
          </div>
          <div style={{ marginTop: 12, fontSize: 12, color: "var(--text-muted)" }}>
            更改数据目录后需重启应用生效。所有书籍、进度、设置将迁移到新目录。
          </div>
        </section>

        {/* Network Sync */}
        <section>
          <h2 style={{ fontSize: 18, marginBottom: 16 }}>网络同步</h2>
          <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 480 }}>
            <div>
              <label style={{ display: "block", fontSize: 13, fontWeight: 500, marginBottom: 6 }}>
                WebDAV 地址
              </label>
              <input
                type="url"
                value={serverUrl}
                onChange={(e) => setField("serverUrl", e.target.value)}
                placeholder="https://dav.example.com"
                style={fieldStyle}
              />
            </div>
            <div>
              <label style={{ display: "block", fontSize: 13, fontWeight: 500, marginBottom: 6 }}>
                用户名
              </label>
              <input
                type="text"
                value={username}
                onChange={(e) => setField("username", e.target.value)}
                placeholder="user"
                style={fieldStyle}
              />
            </div>
            <div>
              <label style={{ display: "block", fontSize: 13, fontWeight: 500, marginBottom: 6 }}>
                密码
              </label>
              <input
                type="password"
                value={passwordDraft}
                onChange={(e) => setField("passwordDraft", e.target.value)}
                placeholder={hasPassword ? "已保存（留空保持原密码）" : ""}
                style={fieldStyle}
              />
            </div>
            <div>
              <label style={{ display: "block", fontSize: 13, fontWeight: 500, marginBottom: 6 }}>
                远端目录
              </label>
              <input
                type="text"
                value={remoteDir}
                onChange={(e) => setField("remoteDir", e.target.value)}
                placeholder="EpubReader"
                style={fieldStyle}
              />
            </div>
          </div>
          <div style={{ marginTop: 8, fontSize: 12, color: "var(--text-muted)" }}>
            {hasPassword ? "密码已保存" : "未保存密码"}
          </div>
          <div style={{ display: "flex", gap: 12, flexWrap: "wrap", marginTop: 16 }}>
            <button
              onClick={saveConfig}
              className="btn btn-primary"
              disabled={testing || previewing || applying}
            >
              保存配置
            </button>
            <button
              onClick={testConnection}
              className="btn btn-secondary"
              disabled={testing || previewing || applying || !serverUrl.trim() || !username.trim()}
            >
              测试连接
            </button>
            <button
              onClick={previewSync}
              className="btn btn-secondary"
              disabled={testing || previewing || applying || !serverUrl.trim() || !username.trim()}
            >
              预览同步
            </button>
            <button
              onClick={applySync}
              className="btn btn-primary"
              disabled={testing || previewing || applying || !preview || unresolvedConflicts > 0}
            >
              执行同步
            </button>
            <button
              onClick={handleClearSync}
              className="btn btn-secondary"
              style={{ color: "#c33" }}
              disabled={testing || previewing || applying}
            >
              清除配置
            </button>
          </div>
          {progress && (
            <div style={{ marginTop: 12, fontSize: 13, color: "var(--text-secondary)" }}>
              {progress.stage}
              {progress.total > 0 ? ` (${progress.current}/${progress.total})` : ""}
            </div>
          )}
          {message && (
            <div style={{ marginTop: 12, fontSize: 13, color: "var(--accent)" }}>{message}</div>
          )}
          {error && (
            <div
              style={{
                marginTop: 12,
                padding: "10px 14px",
                background: "rgba(204, 51, 51, 0.08)",
                color: "#c33",
                borderRadius: 6,
                fontSize: 13,
                wordBreak: "break-all",
              }}
            >
              {error}
              <button
                onClick={clearError}
                style={{
                  marginLeft: 8,
                  color: "#c33",
                  textDecoration: "underline",
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  fontSize: 13,
                }}
              >
                关闭
              </button>
            </div>
          )}
          {preview && (
            <div style={{ marginTop: 16 }}>
              <div style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>
                变更预览 ({preview.actions.length} 项)
              </div>
              {preview.actions.length === 0 ? (
                <div style={{ fontSize: 13, color: "var(--text-muted)" }}>
                  没有需要同步的变更
                </div>
              ) : (
                <div
                  style={{
                    border: "1px solid var(--border-color)",
                    borderRadius: 8,
                    overflow: "hidden",
                  }}
                >
                  {preview.actions.map((action, index) => (
                    <div
                      key={`${action.id}-${index}`}
                      style={{
                        padding: "12px 16px",
                        borderBottom:
                          index === preview.actions.length - 1
                            ? "none"
                            : "1px solid var(--border-color)",
                        background: "var(--card-bg)",
                        display: "flex",
                        flexWrap: "wrap",
                        gap: 12,
                        alignItems: "center",
                        justifyContent: "space-between",
                      }}
                    >
                      <div style={{ minWidth: 220, flex: 1 }}>
                        <div style={{ fontSize: 14, fontWeight: 500, wordBreak: "break-all" }}>
                          {action.label}
                        </div>
                        <div style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 2 }}>
                          {kindLabels[action.kind] ?? action.kind} · {formatBytes(action.size)} ·{" "}
                          {formatTime(action.updated_at)}
                        </div>
                      </div>
                      {action.kind === "conflict" ? (
                        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                          <span style={{ fontSize: 12, color: "#c33" }}>冲突</span>
                          {(["local", "remote"] as const).map((choice) => (
                            <button
                              key={choice}
                              onClick={() => setDecision(action.id, choice)}
                              style={{
                                padding: "6px 12px",
                                borderRadius: 6,
                                fontSize: 13,
                                border: `1px solid ${
                                  decisions[action.id] === choice
                                    ? "var(--accent)"
                                    : "var(--border-color)"
                                }`,
                                background:
                                  decisions[action.id] === choice
                                    ? "var(--accent)"
                                    : "var(--bg-tertiary)",
                                color:
                                  decisions[action.id] === choice
                                    ? "white"
                                    : "var(--text-primary)",
                                cursor: "pointer",
                              }}
                            >
                              {choice === "local" ? "用本地" : "用远端"}
                            </button>
                          ))}
                        </div>
                      ) : (
                        <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
                          {action.direction === "remote" ? "远端 → 本地" : "本地 → 远端"}
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              )}
              {unresolvedConflicts > 0 && (
                <div style={{ marginTop: 8, fontSize: 13, color: "#c33" }}>
                  还有 {unresolvedConflicts} 个冲突未处理
                </div>
              )}
            </div>
          )}
        </section>

        {/* 关于 */}
        <section>
          <h2 style={{ fontSize: 18, marginBottom: 16 }}>关于</h2>
          <div style={{ fontSize: 13, color: "var(--text-secondary)", lineHeight: 1.9 }}>
            <div>轻阅 / QingRead{version ? " · v" + version : ""}</div>
            <div>阅读排版（字号、行距、字体、配色）在阅读页的「阅读设置」面板里调整。</div>
            <div>窗口形态、置顶与低干扰选项在阅读页的「窗口尺寸」面板里调整。</div>
            <div style={{ marginTop: 8, color: "var(--text-muted)" }}>
              数据目录沿用旧版位置，升级后书架、进度与同步凭据不受影响。
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
