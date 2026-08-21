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

export interface ReadingProgress {
  book_id: string;
  chapter_index: number;
  page_in_chapter: number;
  total_pages_read: number;
  last_read: number;
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
