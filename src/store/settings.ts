import { create } from "zustand";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { AppSettings, Theme, CloseBehavior } from "../types";

interface SettingsState {
  settings: AppSettings;
  loaded: boolean;

  loadSettings: () => Promise<void>;
  updateSettings: (partial: Partial<AppSettings>) => Promise<void>;
  setDataDir: (path: string) => Promise<void>;
  restartApp: () => Promise<void>;
}

const defaultSettings: AppSettings = {
  theme: "light",
  font_size: 18,
  line_height: 1.8,
  font_family: "system-ui, -apple-system, sans-serif",
  custom_bg_image: null,
  data_dir: null,
  close_behavior: "quit",
};

export function backgroundImageCss(path: string | null): string | null {
  if (!path) return null;
  return `url("${convertFileSrc(path, "bg-asset")}")`;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: defaultSettings,
  loaded: false,

  loadSettings: async () => {
    try {
      const settings = await invoke<AppSettings>("get_settings");
      set({ settings, loaded: true });
      applySettingsToDOM(settings);
    } catch {
      set({ loaded: true });
    }
  },

  updateSettings: async (partial) => {
    const newSettings = { ...get().settings, ...partial };
    set({ settings: newSettings });
    applySettingsToDOM(partial);
    try {
      await invoke("save_settings", { settings: newSettings });
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  },

  setDataDir: async (path) => {
    const isReset = !path || path.trim() === "";
    const newDataDir = isReset ? null : path;
    set((state) => ({ settings: { ...state.settings, data_dir: newDataDir } }));
    try {
      await invoke("set_data_dir", { path });
    } catch (e) {
      console.error("Failed to set data dir:", e);
      throw e;
    }
  },

  restartApp: async () => {
    await invoke("restart_app");
  },
}));

function applySettingsToDOM(partial: Partial<AppSettings>) {
  if (partial.theme !== undefined) {
    document.documentElement.setAttribute("data-theme", partial.theme);
  }
  if (partial.font_size !== undefined) {
    document.documentElement.style.setProperty("--reader-font-size", `${partial.font_size}px`);
  }
  if (partial.line_height !== undefined) {
    document.documentElement.style.setProperty("--reader-line-height", String(partial.line_height));
  }
  if (partial.font_family !== undefined) {
    document.documentElement.style.setProperty("--reader-font-family", partial.font_family);
  }
  if (partial.custom_bg_image !== undefined) {
    if (partial.custom_bg_image) {
      document.documentElement.style.setProperty(
        "--reader-custom-bg",
        backgroundImageCss(partial.custom_bg_image)
      );
    } else {
      document.documentElement.style.removeProperty("--reader-custom-bg");
    }
  }
}
