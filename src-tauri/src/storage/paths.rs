use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub books_dir: PathBuf,
    pub covers_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub settings_file: PathBuf,
    pub library_file: PathBuf,
    pub progress_dir: PathBuf,
    pub sync_config_file: PathBuf,
    pub sync_state_file: PathBuf,
    pub tombstones_dir: PathBuf,
    /// 本机窗口与低干扰偏好。刻意独立于 settings.json，
    /// 也不参与 WebDAV 同步（同步只覆盖 books/、covers/、progress/、tombstones/）。
    pub window_file: PathBuf,
}

impl AppPaths {
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        // 一律使用用户在 settings.json 里指定的 data_dir，或者回退到
        // %APPDATA%/EpubReader。不再尝试把数据写到 exe 旁 data/，
        // 避免 NSIS 安装到 Program Files 时无写权限导致“看似装好但导入失败”。
        //
        // 兼容说明：产品已改名「轻阅 / QingRead」，但这里的目录名必须继续是
        // EpubReader。它是旧版写入书架、进度、同步凭据的位置，改名会让升级后的
        // 用户看到空书架。同理见 sync/config.rs 里的 SYNC_SERVICE。
        let base = match data_dir {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => dirs::data_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
                .join("EpubReader"),
        };

        let books_dir = base.join("books");
        let covers_dir = base.join("covers");
        let assets_dir = base.join("assets");
        let progress_dir = base.join("progress");
        let settings_file = base.join("settings.json");
        let library_file = base.join("library.json");
        let sync_config_file = base.join("sync_config.json");
        let sync_state_file = base.join("sync_state.json");
        let tombstones_dir = base.join("tombstones");
        let window_file = base.join("window.json");

        Self {
            data_dir: base,
            books_dir,
            covers_dir,
            assets_dir,
            settings_file,
            library_file,
            progress_dir,
            sync_config_file,
            sync_state_file,
            tombstones_dir,
            window_file,
        }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.books_dir)?;
        std::fs::create_dir_all(&self.covers_dir)?;
        std::fs::create_dir_all(&self.assets_dir)?;
        std::fs::create_dir_all(&self.progress_dir)?;
        std::fs::create_dir_all(&self.tombstones_dir)?;
        Ok(())
    }

    pub fn book_path(&self, id: &str, format: &str) -> PathBuf {
        let ext = if format.eq_ignore_ascii_case("txt") { "txt" } else { "epub" };
        self.books_dir.join(format!("{}.{}", id, ext))
    }

    pub fn cover_path(&self, id: &str) -> PathBuf {
        self.covers_dir.join(format!("{}.jpg", id))
    }

    pub fn progress_path(&self, id: &str) -> PathBuf {
        self.progress_dir.join(format!("{}.json", id))
    }

    pub fn custom_bg_path(&self, ext: &str) -> PathBuf {
        let safe_ext = if ext.is_empty() { "img" } else { ext };
        self.assets_dir.join(format!("custom_bg.{}", safe_ext))
    }

    pub fn tombstone_path(&self, id: &str) -> PathBuf {
        self.tombstones_dir.join(format!("{}.json", id))
    }
}

/// 书籍 ID 会参与本地文件名和同步对象名。只接受单个、安全的路径片段，
/// 以免损坏的库文件或远端数据借由 `..` / 分隔符逃出数据目录。
pub fn is_safe_book_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
