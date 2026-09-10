import { create } from "zustand";
import { syncApi } from "../api";
import { useSettingsStore } from "./settings";
import type {
  SyncConfigData,
  SyncDecision,
  SyncPreview,
  SyncSummary,
} from "../types";

export interface SyncProgressPayload {
  stage: string;
  current: number;
  total: number;
}

type SyncField = "serverUrl" | "username" | "passwordDraft" | "remoteDir";

interface SyncState {
  serverUrl: string;
  username: string;
  passwordDraft: string;
  remoteDir: string;
  hasPassword: boolean;
  loaded: boolean;
  testing: boolean;
  previewing: boolean;
  applying: boolean;
  preview: SyncPreview | null;
  decisions: Record<string, "local" | "remote">;
  progress: SyncProgressPayload | null;
  message: string | null;
  error: string | null;

  loadConfig: () => Promise<void>;
  setField: (field: SyncField, value: string) => void;
  saveConfig: () => Promise<boolean>;
  clearConfig: () => Promise<void>;
  testConnection: () => Promise<void>;
  previewSync: () => Promise<void>;
  setDecision: (id: string, choice: "local" | "remote") => void;
  applySync: () => Promise<void>;
  clearError: () => void;
}

export const useSyncStore = create<SyncState>((set, get) => ({
  serverUrl: "",
  username: "",
  passwordDraft: "",
  remoteDir: "",
  hasPassword: false,
  loaded: false,
  testing: false,
  previewing: false,
  applying: false,
  preview: null,
  decisions: {},
  progress: null,
  message: null,
  error: null,

  loadConfig: async () => {
    try {
      const config = await syncApi.getConfig();
      set({
        serverUrl: config.server_url,
        username: config.username,
        remoteDir: config.remote_dir,
        hasPassword: config.has_password,
        passwordDraft: "",
        loaded: true,
      });
    } catch (e) {
      set({ loaded: true, error: String(e) });
    }
  },

  setField: (field, value) => {
    if (field === "serverUrl") {
      set({ serverUrl: value, preview: null, decisions: {}, progress: null, message: null });
    } else if (field === "username") {
      set({ username: value, preview: null, decisions: {}, progress: null, message: null });
    } else if (field === "passwordDraft") {
      set({ passwordDraft: value });
    } else {
      set({ remoteDir: value, preview: null, decisions: {}, progress: null, message: null });
    }
  },

  saveConfig: async () => {
    const { serverUrl, username, passwordDraft, remoteDir } = get();
    if (!serverUrl.trim() || !username.trim()) {
      set({ error: "请填写 WebDAV 地址和用户名" });
      return false;
    }
    set({ error: null, message: null });
    try {
      await syncApi.setConfig({
        serverUrl: serverUrl.trim(),
        username: username.trim(),
        password: passwordDraft,
        remoteDir: remoteDir.trim(),
      });
      const config = await syncApi.getConfig();
      set({
        serverUrl: config.server_url,
        username: config.username,
        remoteDir: config.remote_dir,
        hasPassword: config.has_password,
        passwordDraft: "",
        message: "同步配置已保存",
      });
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    }
  },

  clearConfig: async () => {
    set({ error: null, message: null });
    try {
      await syncApi.clearConfig();
      set({
        serverUrl: "",
        username: "",
        passwordDraft: "",
        remoteDir: "",
        hasPassword: false,
        preview: null,
        decisions: {},
        progress: null,
        message: "已清除同步配置",
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  testConnection: async () => {
    const { passwordDraft } = get();
    set({ error: null, message: null, testing: true });
    const saved = await get().saveConfig();
    if (!saved) {
      set({ testing: false });
      return;
    }
    try {
      await syncApi.testConnection(passwordDraft.trim() ? passwordDraft.trim() : null);
      set({ testing: false, message: "WebDAV 连接成功" });
    } catch (e) {
      set({ testing: false, error: String(e) });
    }
  },

  previewSync: async () => {
    set({ error: null, message: null, previewing: true, preview: null, decisions: {} });
    const saved = await get().saveConfig();
    if (!saved) {
      set({ previewing: false });
      return;
    }
    try {
      const preview = await syncApi.preview();
      set({ preview, previewing: false });
    } catch (e) {
      set({ previewing: false, error: String(e) });
    }
  },

  setDecision: (id, choice) => {
    set((state) => ({
      decisions: { ...state.decisions, [id]: choice },
    }));
  },

  applySync: async () => {
    const { preview, decisions } = get();
    if (!preview) return;
    const conflicts = preview.actions.filter((action) => action.kind === "conflict");
    if (conflicts.some((action) => !decisions[action.id])) {
      set({ error: "请先解决所有冲突" });
      return;
    }
    const syncDecisions: SyncDecision[] = conflicts.map((action) => ({
      id: action.id,
      choice: decisions[action.id],
    }));
    set({ error: null, message: null, applying: true, progress: null });
    try {
      const summary = await syncApi.apply(syncDecisions);
      set({
        applying: false,
        progress: null,
        preview: null,
        decisions: {},
        message: summary.message,
      });
      useSettingsStore.getState().loadSettings();
    } catch (e) {
      set({ applying: false, progress: null, error: String(e) });
    }
  },

  clearError: () => {
    set({ error: null });
  },
}));
