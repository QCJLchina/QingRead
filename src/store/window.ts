import { create } from "zustand";
import { windowApi } from "../api";
import { clampWindowSize, presetById } from "../lib/window-presets";
import type { WindowLayoutId, WindowSettings } from "../types";

/** 阅读页里的浮层。同一时刻只开一个。 */
export type ReaderPanel = "none" | "reading" | "window" | "search";

export const defaultWindowSettings: WindowSettings = {
  layout: "standard",
  width: 940,
  height: 610,
  x: null,
  y: null,
  lock_ratio: false,
  always_on_top: false,
  low_distraction: false,
  hide_hotkey: "Ctrl+Alt+H",
  toolbar_auto_hide: false,
  blur_curtain: false,
};

interface WindowState {
  settings: WindowSettings;
  loaded: boolean;
  /** 正文可用宽度不足 600px，目录与工具栏改为浮层 / 精简形态 */
  narrow: boolean;
  /** 目录栏是否展开（宽窗口是固定栏，窄窗口是浮层） */
  sidebarOpen: boolean;
  panel: ReaderPanel;
  /** 中性遮挡页是否覆盖正文 */
  curtain: boolean;
  /** 全局快捷键注册情况，用于提示用户 */
  hotkeyNotice: string | null;
  toolbarHidden: boolean;

  load: () => Promise<void>;
  update: (partial: Partial<WindowSettings>) => Promise<void>;
  applyPreset: (id: WindowLayoutId) => Promise<void>;
  applySize: (width: number, height: number) => Promise<void>;
  setNarrow: (narrow: boolean) => void;
  toggleSidebar: () => void;
  closeSidebar: () => void;
  openPanel: (panel: ReaderPanel) => void;
  closePanel: () => void;
  setCurtain: (on: boolean) => void;
  toggleCurtain: () => void;
  setHotkeyNotice: (notice: string | null) => void;
  setToolbarHidden: (hidden: boolean) => void;
}

/** 屏幕可用区域（CSS 像素）。WebView 里 screen 报的就是当前显示器。 */
export function screenWorkArea(): { width: number; height: number } {
  const width = typeof window === "undefined" ? 1920 : window.screen.availWidth;
  const height = typeof window === "undefined" ? 1080 : window.screen.availHeight;
  return {
    width: width > 0 ? width : 1920,
    height: height > 0 ? height : 1080,
  };
}

export const useWindowStore = create<WindowState>((set, get) => ({
  settings: defaultWindowSettings,
  loaded: false,
  narrow: false,
  sidebarOpen: true,
  panel: "none",
  curtain: false,
  hotkeyNotice: null,
  toolbarHidden: false,

  load: async () => {
    try {
      const settings = await windowApi.getSettings();
      set({
        settings: { ...defaultWindowSettings, ...settings },
        loaded: true,
        sidebarOpen: settings.layout === "standard" || settings.layout === "custom",
      });
    } catch (error) {
      console.error("加载窗口设置失败:", error);
      set({ loaded: true });
    }
  },

  update: async (partial) => {
    const settings = { ...get().settings, ...partial };
    set({ settings });
    try {
      await windowApi.saveSettings(settings);
    } catch (error) {
      console.error("保存窗口设置失败:", error);
    }
  },

  applyPreset: async (id) => {
    const preset = presetById(id);
    if (!preset) return;
    const size = clampWindowSize(preset.width, preset.height, screenWorkArea());
    const previous = get().settings;
    const lockRatio = id === "custom" ? previous.lock_ratio : false;
    set({
      settings: {
        ...previous,
        layout: id,
        width: size.width,
        height: size.height,
        lock_ratio: lockRatio,
      },
      sidebarOpen: id === "standard",
      panel: "none",
    });
    try {
      await windowApi.applyLayout({
        width: size.width,
        height: size.height,
        layout: id,
        lockRatio,
      });
    } catch (error) {
      console.error("应用窗口预设失败:", error);
    }
  },

  applySize: async (width, height) => {
    const size = clampWindowSize(width, height, screenWorkArea());
    const previous = get().settings;
    set({ settings: { ...previous, layout: "custom", width: size.width, height: size.height } });
    try {
      await windowApi.applyLayout({
        width: size.width,
        height: size.height,
        layout: "custom",
        lockRatio: previous.lock_ratio,
      });
    } catch (error) {
      console.error("应用窗口尺寸失败:", error);
    }
  },

  setNarrow: (narrow) => {
    if (get().narrow === narrow) return;
    set({ narrow, sidebarOpen: narrow ? false : get().sidebarOpen });
  },

  toggleSidebar: () => {
    const { narrow, sidebarOpen, panel } = get();
    if (narrow) {
      // 窄窗口里目录是浮层，和设置面板互斥
      const visible = sidebarOpen && panel === "none";
      set({ sidebarOpen: !visible, panel: "none" });
      return;
    }
    set({ sidebarOpen: !sidebarOpen });
  },

  closeSidebar: () => set({ sidebarOpen: false }),

  openPanel: (panel) => {
    const next = get().panel === panel ? "none" : panel;
    set({ panel: next, sidebarOpen: next === "none" ? get().sidebarOpen : false });
  },

  closePanel: () => set({ panel: "none" }),

  setCurtain: (on) => set({ curtain: on }),

  toggleCurtain: () => set({ curtain: !get().curtain }),

  setHotkeyNotice: (notice) => set({ hotkeyNotice: notice }),

  setToolbarHidden: (hidden) => set({ toolbarHidden: hidden }),
}));
