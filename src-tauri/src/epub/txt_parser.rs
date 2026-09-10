use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

/// 章节元信息（不含内容，快速扫描得到）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxtChapterMeta {
    pub index: usize,
    pub title: String,
    pub byte_start: usize,  // 在标准化文本中的起始字节偏移
    pub byte_end: usize,    // 在标准化文本中的结束字节偏移
    pub char_count: usize,  // 字符数（用于估算页数）
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_chapter_markers() {
        assert!(TxtParser::is_chapter_marker("第一章"));
        assert!(TxtParser::is_chapter_marker("第12章 标题"));
        assert!(TxtParser::is_chapter_marker("Chapter 1"));
        assert!(TxtParser::is_chapter_marker("尾声"));
        assert!(!TxtParser::is_chapter_marker("普通段落"));
        assert!(!TxtParser::is_chapter_marker(""));
    }

    #[test]
    fn decodes_utf16_segments() {
        let le = [0xFF, 0xFE, b'h', 0, b'i', 0];
        assert_eq!(TxtParser::decode_segment(&le, "utf16le", 2), "hi");

        let be = [0xFE, 0xFF, 0, b'h', 0, b'i'];
        assert_eq!(TxtParser::decode_segment(&be, "utf16be", 2), "hi");
    }

    #[test]
    fn escapes_text_as_html() {
        assert_eq!(
            TxtParser::text_to_html("hello & <world>"),
            "<p>hello &amp; &lt;world&gt;</p>\n"
        );
        assert_eq!(TxtParser::text_to_html("a\n\nb"), "<p>a</p>\n<p>b</p>\n");
    }

    #[test]
    fn unmarked_txt_chunks_match_quick_scan_and_load() {
        let path = std::env::temp_dir()
            .join(format!("qingread-txt-unmarked-{}.txt", std::process::id()));
        let body = "测".repeat(120_000);
        std::fs::write(&path, format!("书名：测试\n作者：某人\n{}", body)).unwrap();

        let index = TxtParser::quick_scan(&path).unwrap();
        assert_eq!(index.chapters.len(), 3);
        assert!(index.chapters.iter().all(|c| c.char_count > 0));
        assert_eq!(index.chapters[0].byte_end, index.chapters[1].byte_start);
        assert_eq!(index.chapters[1].byte_end, index.chapters[2].byte_start);

        let middle = TxtParser::load_chapter(&path, 1).unwrap();
        assert!(middle.content.contains("测"));
        assert!(!middle.content.is_empty());
    }

    #[test]
    fn single_marker_txt_is_one_chapter() {
        let path = std::env::temp_dir()
            .join(format!("qingread-txt-single-{}.txt", std::process::id()));
        std::fs::write(&path, "书名：测试\n第一章\n正文内容\n").unwrap();

        let index = TxtParser::quick_scan(&path).unwrap();
        assert_eq!(index.chapters.len(), 1);

        let chapter = TxtParser::load_chapter(&path, 0).unwrap();
        assert_eq!(chapter.title, "第一章");
        assert!(chapter.content.contains("正文内容"));
    }

    #[test]
    fn marked_txt_uses_exact_shared_boundaries() {
        let path = std::env::temp_dir()
            .join(format!("qingread-txt-marked-{}.txt", std::process::id()));
        std::fs::write(
            &path,
            "书名：测试\n作者：某人\n第一章\n开头\n第二章\n中间\n第三章\n结尾\n",
        )
        .unwrap();

        let index = TxtParser::quick_scan(&path).unwrap();
        assert_eq!(index.chapters.len(), 3);
        assert_eq!(index.chapters[0].byte_end, index.chapters[1].byte_start);
        assert_eq!(index.chapters[1].byte_end, index.chapters[2].byte_start);

        let first = TxtParser::load_chapter(&path, 0).unwrap();
        assert_eq!(first.title, "第一章");
        assert!(first.content.contains("开头"));
        assert!(!first.content.contains("第二章"));

        let last = TxtParser::load_chapter(&path, 2).unwrap();
        assert_eq!(last.title, "第三章");
        assert!(last.content.contains("结尾"));
    }
}

/// 快速扫描得到的书本索引
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxtIndex {
    pub title: String,
    pub author: String,
    pub chapters: Vec<TxtChapterMeta>,
    pub encoding: String,   // "utf8" | "utf16le" | "utf16be"
    pub bom_offset: usize,  // BOM 之后的偏移
    pub total_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxtChapter {
    pub index: usize,
    pub title: String,
    pub content: String,  // HTML 格式
}

/// 全局 TXT 索引缓存（快速扫描结果）
static TXT_CACHE: Lazy<Mutex<std::collections::HashMap<String, Arc<TxtIndex>>>> =
    Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

pub struct TxtParser;

impl TxtParser {
    /// 快速扫描：只找章节标记和字符数，不转换 HTML
    /// 结果全部缓存，后续调用毫秒级
    pub fn quick_scan(path: &Path) -> Result<Arc<TxtIndex>> {
        let cache_key = path.to_string_lossy().to_string();
        {
            let cache = TXT_CACHE.lock().unwrap();
            if let Some(idx) = cache.get(&cache_key) {
                return Ok(Arc::clone(idx));
            }
        }

        let bytes = fs::read(path)
            .map_err(|e| anyhow!("Failed to read file: {}", e))?;

        // 检测编码 + 去 BOM
        let (encoding, bom_offset) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            ("utf8", 3)
        } else if bytes.starts_with(&[0xFF, 0xFE]) {
            ("utf16le", 2)
        } else if bytes.starts_with(&[0xFE, 0xFF]) {
            ("utf16be", 2)
        } else {
            ("utf8", 0)
        };

        let content = Self::decode_segment(&bytes, encoding, bom_offset);
        let normalized = content.replace("\r\n", "\n").replace('\r', "\n");

        // 提取标题/作者（只读首 30 行）
        let (title, author, body_start_line) = Self::extract_meta_from_lines(&normalized, path);

        // 找章节标记
        let chapter_markers = Self::find_chapter_markers_from_lines(&normalized, body_start_line);

        // 计算章节字节边界并写入索引；load_chapter 直接复用这些边界，
        // 避免快速扫描和懒加载各自估算导致章节数/边界不一致。
        let line_starts = Self::line_starts(&normalized);
        let mut chapters = Vec::new();
        let mut total_chars = 0usize;

        if !chapter_markers.is_empty() {
            for (i, &(line_idx, ref title)) in chapter_markers.iter().enumerate() {
                let byte_start = line_starts.get(line_idx).copied().unwrap_or(0);
                let byte_end = if i + 1 < chapter_markers.len() {
                    line_starts
                        .get(chapter_markers[i + 1].0)
                        .copied()
                        .unwrap_or(normalized.len())
                } else {
                    normalized.len()
                };
                let slice = &normalized[byte_start..byte_end];
                let char_count = slice.chars().count();
                total_chars += char_count;
                chapters.push(TxtChapterMeta {
                    index: i,
                    title: title.clone(),
                    byte_start,
                    byte_end,
                    char_count,
                });
            }
        } else {
            // 无章节标记：从正文起始处按固定字符数拆块
            let chunk_size = 50000;
            let body_start = line_starts.get(body_start_line).copied().unwrap_or(0);
            let mut start = body_start;
            let mut index = 0;
            while start < normalized.len() {
                let end = normalized[start..]
                    .char_indices()
                    .nth(chunk_size)
                    .map(|(offset, _)| start + offset)
                    .unwrap_or(normalized.len());
                let slice = &normalized[start..end];
                let char_count = slice.chars().count();
                total_chars += char_count;
                chapters.push(TxtChapterMeta {
                    index,
                    title: format!("第 {} 部分", index + 1),
                    byte_start: start,
                    byte_end: end,
                    char_count,
                });
                start = end;
                index += 1;
            }
            if chapters.is_empty() {
                chapters.push(TxtChapterMeta {
                    index: 0,
                    title: "第 1 部分".to_string(),
                    byte_start: body_start,
                    byte_end: body_start,
                    char_count: 0,
                });
            }
        }

        let index = Arc::new(TxtIndex {
            title,
            author,
            chapters,
            encoding: encoding.to_string(),
            bom_offset,
            total_chars,
        });

        TXT_CACHE.lock().unwrap().insert(cache_key, Arc::clone(&index));
        Ok(index)
    }

    /// 懒加载：读取并转换单个章节为 HTML
    pub fn load_chapter(path: &Path, chapter_index: usize) -> Result<TxtChapter> {
        let index = Self::quick_scan(path)?;

        if chapter_index >= index.chapters.len() {
            return Err(anyhow!("Chapter index out of bounds: {}", chapter_index));
        }

        let bytes = fs::read(path)
            .map_err(|e| anyhow!("Failed to read file: {}", e))?;

        let content = Self::decode_segment(&bytes, &index.encoding, index.bom_offset);
        let normalized = content.replace("\r\n", "\n").replace('\r', "\n");

        let meta = &index.chapters[chapter_index];
        let start = meta.byte_start.min(normalized.len());
        let end = meta.byte_end.min(normalized.len()).max(start);
        let content = Self::text_to_html(&normalized[start..end]);

        Ok(TxtChapter {
            index: chapter_index,
            title: meta.title.clone(),
            content,
        })
    }

    /// 清除指定文件的快速扫描缓存，供文件被替换或删除时调用。
    pub fn invalidate(path: &Path) {
        TXT_CACHE
            .lock()
            .unwrap()
            .remove(&path.to_string_lossy().to_string());
    }

    // ---- 内部辅助方法 ----

    /// 每行（含行尾换行符）在标准化文本中的起始字节偏移。
    fn line_starts(normalized: &str) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut offset = 0;
        for line in normalized.split_inclusive('\n') {
            starts.push(offset);
            offset += line.len();
        }
        starts
    }

    fn decode_segment(bytes: &[u8], encoding: &str, _bom: usize) -> String {
        let data = &bytes[_bom..];
        match encoding {
            "utf16le" => {
                data.chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .filter_map(|c| char::from_u32(c as u32))
                    .collect()
            }
            "utf16be" => {
                data.chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .filter_map(|c| char::from_u32(c as u32))
                    .collect()
            }
            _ => String::from_utf8_lossy(data).to_string(),
        }
    }

    fn extract_meta_from_lines(content: &str, path: &Path) -> (String, String, usize) {
        let mut title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("未知书名")
            .to_string();
        let mut author = String::new();
        let mut body_start = 0;

        for (i, line) in content.lines().take(30).enumerate() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed
                .strip_prefix("书名:")
                .or_else(|| trimmed.strip_prefix("书名："))
            {
                if !rest.trim().is_empty() {
                    title = rest.trim().to_string();
                    body_start = i + 1;
                    continue;
                }
            }
            if let Some(rest) = trimmed
                .strip_prefix("作者:")
                .or_else(|| trimmed.strip_prefix("作者："))
            {
                if !rest.trim().is_empty() {
                    author = rest.trim().to_string();
                    body_start = i + 1;
                    continue;
                }
            }
            if trimmed.starts_with('《') && trimmed.ends_with('》') && trimmed.len() < 100 {
                title = trimmed.trim_start_matches('《').trim_end_matches('》').to_string();
                body_start = i + 1;
            }
        }
        (title, author, body_start)
    }

    fn find_chapter_markers_from_lines(content: &str, skip: usize) -> Vec<(usize, String)> {
        let mut markers = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            if idx < skip {
                continue;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.len() > 80 {
                continue;
            }
            if Self::is_chapter_marker(trimmed) {
                markers.push((idx, trimmed.to_string()));
            }
        }
        markers
    }

    fn is_chapter_marker(line: &str) -> bool {
        if (line.starts_with("Chapter ") || line.starts_with("CHAPTER ")) && line.len() < 60 {
            return true;
        }

        // 第N章/回/节/卷/集/篇
        if line.starts_with("第") {
            let rest = line.trim_start_matches('第');
            // 跳过空白和数字
            let num_end = rest.find(|c: char| !c.is_ascii_digit() && !c.is_whitespace()
                && !matches!(c, '零'|'一'|'二'|'三'|'四'|'五'|'六'|'七'|'八'|'九'|'十'|'百'|'千'|'万'|'亿'));
            if let Some(idx) = num_end {
                let suffix = &rest[idx..];
                // 检查后缀第一个字符
                if suffix.starts_with('章') || suffix.starts_with('回') || suffix.starts_with('节')
                    || suffix.starts_with('卷') || suffix.starts_with('集') || suffix.starts_with('篇')
                {
                    return true;
                }
            }
        }

        let special = ["序章", "序", "楔子", "终章", "尾声", "后记", "前言", "引子", "引言"];
        if special.iter().any(|s| line == *s) { return true; }
        if line.starts_with("番外") && line.len() <= 20 { return true; }

        false
    }

    fn text_to_html(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 64);
        let mut in_paragraph = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if in_paragraph { out.push_str("</p>\n"); in_paragraph = false; }
            } else {
                if !in_paragraph { out.push_str("<p>"); in_paragraph = true; }
                else { out.push_str("<br>"); }
                for c in trimmed.chars() {
                    match c {
                        '&' => out.push_str("&amp;"),
                        '<' => out.push_str("&lt;"),
                        '>' => out.push_str("&gt;"),
                        '"' => out.push_str("&quot;"),
                        _ => out.push(c),
                    }
                }
            }
        }
        if in_paragraph { out.push_str("</p>\n"); }
        out
    }
}
