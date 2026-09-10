import { create } from "zustand";
import { convertFileSrc } from "@tauri-apps/api/core";
import { settingsApi } from "../api";
import type { AppSettings, CloseBehavior, ReaderMode, Theme } from "../types";

interface SettingsState {
  settings: AppSettings;
  loaded: boolean;

  loadSettings: () => Promise<void>;
  updateSettings: (partial: Partial<AppSettings>) => Promise<void>;
  /** 记录用户选择的阅读模式；一旦选过就不再回落到默认值 */
  setReadingMode: (mode: ReaderMode) => Promise<void>;
  setDataDir: (path: string) => Promise<void>;
  restartApp: () => Promise<void>;
}

export const defaultSettings: AppSettings = {
  theme: "light",
  font_size: 18,
  line_height: 1.8,
  font_family: "system-ui, -apple-system, sans-serif",
  custom_bg_image: null,
  data_dir: null,
  close_behavior: "quit",
  // null 表示用户从未选择过：升级上来的老安装保持滚动习惯（见 resolveReadingMode）
  reading_mode: null,
  content_width: null,
  content_padding: null,
};

/**
 * 决定本次会话使用哪种阅读模式。
 *
 * null 表示老用户从没选过：保持原来的滚动习惯，不硬把界面换成分页。
 * 真正的新安装由后端在首次运行时写入 paged。
 */
export function resolveReadingMode(mode: ReaderMode | null): ReaderMode {
  return mode === "paged" || mode === "scroll" ? mode : "scroll";
}

export function resolveContentPadding(padding: number | null, mode: ReaderMode): number {
  if (typeof padding === "number" && padding >= 0) return padding;
  return mode === "paged" ? 32 : 40;
}

export function backgroundImageCss(path: string | null): string | null {
  if (!path) return null;
  return 'url("' + convertFileSrc(path, "bg-asset") + '")';
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: defaultSettings,
  loaded: false,

  loadSettings: async () => {
    try {
      const settings = await settingsApi.get();
      const merged = { ...defaultSettings, ...settings };
      set({ settings: merged, loaded: true });
      applySettingsToDOM(merged);
    } catch {
      set({ loaded: true });
    }
  },

  updateSettings: async (partial) => {
    const settings = { ...get().settings, ...partial };
    set({ settings });
    applySettingsToDOM(partial);
    try {
      await settingsApi.save(settings);
    } catch (error) {
      console.error("Failed to save settings:", error);
    }
  },

  setReadingMode: async (mode) => {
    await get().updateSettings({ reading_mode: mode });
  },

  setDataDir: async (path) => {
    const isReset = !path || path.trim() === "";
    const newDataDir = isReset ? null : path;
    set((state) => ({ settings: { ...state.settings, data_dir: newDataDir } }));
    try {
      await settingsApi.setDataDir(path);
    } catch (error) {
      console.error("Failed to set data dir:", error);
      throw error;
    }
  },

  restartApp: async () => {
    await settingsApi.restart();
  },
}));

function applySettingsToDOM(partial: Partial<AppSettings>) {
  if (partial.theme !== undefined) {
    document.documentElement.setAttribute("data-theme", partial.theme);
  }
  if (partial.font_size !== undefined) {
    document.documentElement.style.setProperty("--reader-font-size", partial.font_size + "px");
  }
  if (partial.line_height !== undefined) {
    document.documentElement.style.setProperty("--reader-line-height", String(partial.line_height));
  }
  if (partial.font_family !== undefined) {
    document.documentElement.style.setProperty("--reader-font-family", partial.font_family);
  }
  if (partial.content_width !== undefined) {
    const width = partial.content_width;
    if (typeof width === "number" && width > 0) {
      document.documentElement.style.setProperty("--reader-content-width", width + "px");
    } else {
      document.documentElement.style.removeProperty("--reader-content-width");
    }
  }
  if (partial.content_padding !== undefined) {
    const padding = partial.content_padding;
    if (typeof padding === "number" && padding >= 0) {
      document.documentElement.style.setProperty("--reader-padding", padding + "px");
    } else {
      document.documentElement.style.removeProperty("--reader-padding");
    }
  }
  if (partial.custom_bg_image !== undefined) {
    const css = backgroundImageCss(partial.custom_bg_image);
    if (css) {
      document.documentElement.style.setProperty("--reader-custom-bg", css);
    } else {
      document.documentElement.style.removeProperty("--reader-custom-bg");
    }
  }
}

export const THEME_CYCLE: Theme[] = ["light", "dark", "sepia", "green"];
export const THEME_LABELS: Record<Theme, string> = {
  light: "明亮",
  dark: "暗黑",
  sepia: "羊皮纸",
  green: "护眼绿",
};
export const CLOSE_BEHAVIOR_LABELS: Record<CloseBehavior, string> = {
  quit: "直接退出",
  minimize_to_tray: "最小化到托盘",
};
