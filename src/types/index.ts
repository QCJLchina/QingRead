export interface BookInfo {
  id: string;
  title: string;
  author: string;
  cover: string | null;
  file_path: string;
  added_at: number;
  file_size: number;
  format: string;
}

export interface ChapterData {
  index: number;
  title: string;
  content: string;
}

export interface TocEntry {
  title: string;
  href: string;
  level: number;
  chapter_index: number | null;
}

export type ReaderMode = "paged" | "scroll";

/**
 * 稳定阅读位置：指向清洗后正文里的某个字符，而不是某一页。
 * 窗口比例、字号、行距变化后页码会变，这个锚点不会。
 */
export interface TextAnchor {
  /** 从内容根节点出发的子节点索引链 */
  path: number[];
  /** 该文本节点内的字符偏移 */
  offset: number;
  /** 附近文本，路径失效时用于重新定位 */
  snippet: string;
  /** 章节内比例（0-1），最后的回退方案 */
  ratio: number;
}

export interface ReadingProgress {
  book_id: string;
  chapter_index: number;
  page_in_chapter: number;
  total_pages_read: number;
  last_read: number;
  /** 新增：稳定位置。旧的进度文件没有该字段。 */
  anchor?: TextAnchor | null;
  /** 新增：上次使用的阅读模式 */
  mode?: ReaderMode | null;
}

export type Theme = "light" | "dark" | "sepia" | "green";
export type CloseBehavior = "quit" | "minimize_to_tray";

export interface AppSettings {
  theme: Theme;
  font_size: number;
  line_height: number;
  font_family: string;
  custom_bg_image: string | null;
  data_dir: string | null;
  close_behavior: CloseBehavior;
  /**
   * 阅读模式。null 表示用户从未选择过：
   * 升级上来的老安装保持滚动习惯，全新安装由后端写入 paged。
   */
  reading_mode: ReaderMode | null;
  /** 正文栏最大宽度（逻辑像素），null / 0 表示跟随窗口 */
  content_width: number | null;
  /** 正文内边距（逻辑像素） */
  content_padding: number | null;
}

export type WindowLayoutId = "standard" | "slim" | "strip" | "mini" | "custom";

/**
 * 本机窗口与低干扰偏好。存在独立的 window.json，不参与 WebDAV 同步。
 */
export interface WindowSettings {
  layout: WindowLayoutId;
  /** 客户区宽高，逻辑像素 */
  width: number;
  height: number;
  x: number | null;
  y: number | null;
  lock_ratio: boolean;
  always_on_top: boolean;
  low_distraction: boolean;
  /** 全局隐藏/恢复快捷键，形如 Ctrl+Alt+H */
  hide_hotkey: string;
  toolbar_auto_hide: boolean;
  blur_curtain: boolean;
}

export interface SyncConfigData {
  server_url: string;
  username: string;
  remote_dir: string;
  has_password: boolean;
}

export interface SyncAction {
  id: string;
  kind: "upload" | "download" | "delete" | "conflict";
  label: string;
  size: number;
  updated_at: number;
  local_exists: boolean;
  remote_exists: boolean;
  conflict_type: string | null;
  direction: string;
}

export interface SyncPreview {
  actions: SyncAction[];
  has_remote: boolean;
}

export interface SyncSummary {
  applied: number;
  message: string;
}

export interface SyncDecision {
  id: string;
  choice: "local" | "remote";
}
