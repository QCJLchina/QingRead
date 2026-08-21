use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;

#[cfg(test)]
mod parser_tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookMetadata {
    pub title: String,
    pub author: String,
    pub language: String,
    pub publisher: String,
    pub description: String,
    pub cover_href: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub index: usize,
    pub title: String,
    pub href: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    pub level: usize,
}

/// 轻量级书籍结构（不预加载资源）
#[derive(Debug, Clone)]
pub struct BookIndex {
    pub metadata: BookMetadata,
    pub spine: Vec<String>,           // 顺序的 manifest id
    pub manifest: HashMap<String, ManifestItem>,
    pub toc: Vec<TocEntry>,
    pub opf_dir: String,
    pub cover_data_uri: Option<String>,
    /// 每章估算的字符数（从 zip entry size 推算，乘以 0.8 文本占比）
    pub estimated_chars: Vec<usize>,
}

/// 全局书籍缓存
static BOOK_CACHE: Lazy<Mutex<HashMap<String, Arc<BookIndex>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// 章节 HTML 缓存：(book_path, chapter_index) -> 已解析的 ChapterInfo。
/// 避免每次切章都重新打开 zip + 提取 + sanitize，对大 EPUB 提升极大。
/// 使用 LRU 淘汰策略，防止厚书（300+ 章）导致无限内存增长。
const CHAPTER_CACHE_CAPACITY: usize = 64;

struct ChapterCache {
    map: HashMap<(String, usize), Arc<ChapterInfo>>,
    order: VecDeque<(String, usize)>,
}

impl ChapterCache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &(String, usize)) -> Option<Arc<ChapterInfo>> {
        if let Some(v) = self.map.get(key) {
            let result = Arc::clone(v);
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.clone());
            Some(result)
        } else {
            None
        }
    }

    fn insert(&mut self, key: (String, usize), value: Arc<ChapterInfo>) {
        if self.map.contains_key(&key) {
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
        }
        self.map.insert(key.clone(), value);
        self.order.push_back(key);
        while self.map.len() > CHAPTER_CACHE_CAPACITY {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }
}

static CHAPTER_CACHE: Lazy<Mutex<ChapterCache>> = Lazy::new(|| Mutex::new(ChapterCache::new()));

pub struct EpubParser;

impl EpubParser {
    fn read_zip_entry<R: Read + std::io::Seek>(
        archive: &mut zip::ZipArchive<R>,
        name: &str,
    ) -> Result<String> {
        let mut file = archive.by_name(name).map_err(|_| anyhow!("Entry not found: {}", name))?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Ok(content)
    }

    fn read_zip_entry_bytes<R: Read + std::io::Seek>(
        archive: &mut zip::ZipArchive<R>,
        name: &str,
    ) -> Result<Vec<u8>> {
        let mut file = archive.by_name(name).map_err(|_| anyhow!("Entry not found: {}", name))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn parse_container(xml: &str) -> Result<String> {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();
        let mut opf_path = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                    if e.name().as_ref() == b"rootfile" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"full-path" {
                                opf_path = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow!("XML parse error in container: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        opf_path.ok_or_else(|| anyhow!("No rootfile found in container.xml"))
    }

    fn parse_opf(xml: &str) -> Result<(BookMetadata, HashMap<String, ManifestItem>, Vec<String>)> {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();

        let mut metadata = BookMetadata {
            title: String::new(),
            author: String::new(),
            language: String::new(),
            publisher: String::new(),
            description: String::new(),
            cover_href: None,
        };

        let mut manifest = HashMap::new();
        let mut spine = Vec::new();
        let mut in_metadata = false;
        let mut in_manifest = false;
        let mut in_spine = false;
        let mut current_tag = String::new();
        let mut cover_id: Option<String> = None;

        // First pass: parse metadata, manifest, spine
        // 注意：<item /> 和 <itemref /> 是自闭合标签，触发 Event::Empty 而非 Event::Start，
        // 两者都必须处理，否则 manifest 和 spine 永远为空。
        loop {
            let event = reader.read_event_into(&mut buf);
            let e = match &event {
                Ok(Event::Start(e)) => Some((e, false)),
                Ok(Event::Empty(e)) => Some((e, true)),
                Ok(Event::Text(ref e)) => {
                    if in_metadata {
                        let text = e.unescape().unwrap_or_default().to_string();
                        match current_tag.as_str() {
                            "dc:title" | "title" => metadata.title = text,
                            "dc:creator" | "creator" => metadata.author = text,
                            "dc:language" | "language" => metadata.language = text,
                            "dc:publisher" | "publisher" => metadata.publisher = text,
                            "dc:description" | "description" => metadata.description = text,
                            _ => {}
                        }
                    }
                    buf.clear();
                    continue;
                }
                Ok(Event::End(ref e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "metadata" | "dc:metadata" | "opf:metadata" => in_metadata = false,
                        "manifest" => in_manifest = false,
                        "spine" => in_spine = false,
                        _ => {}
                    }
                    current_tag.clear();
                    buf.clear();
                    continue;
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow!("XML parse error in OPF: {}", e)),
                _ => { buf.clear(); continue; }
            };

            if let Some((e, _is_empty)) = e {
                // 去掉命名空间前缀（如 opf:itemref → itemref）
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = if let Some(p) = tag.rfind(':') { tag[p+1..].to_string() } else { tag.clone() };

                // 只有非自闭合标签才切换节上下文
                if !_is_empty {
                    match local.as_str() {
                        "metadata" => in_metadata = true,
                        "manifest" => in_manifest = true,
                        "spine" => in_spine = true,
                        _ => {}
                    }
                }

                if in_metadata {
                    current_tag = local.clone();
                    if local == "meta" {
                        let mut meta_name = String::new();
                        let mut meta_property = String::new();
                        let mut meta_content = None;
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            match key.as_str() {
                                "name" => meta_name = val,
                                "property" => meta_property = val,
                                "content" => meta_content = Some(val),
                                _ => {}
                            }
                        }
                        if cover_id.is_none()
                            && (meta_name == "cover" || meta_property == "cover")
                        {
                            cover_id = meta_content;
                        }
                    }
                }

                if in_manifest && local == "item" {
                    let mut id = String::new();
                    let mut href = String::new();
                    let mut media_type = String::new();
                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        match key.as_str() {
                            "id" => id = val,
                            "href" => href = val,
                            "media-type" => media_type = val,
                            _ => {}
                        }
                    }
                    if !id.is_empty() && !href.is_empty() {
                        manifest.insert(id.clone(), ManifestItem { id, href, media_type });
                    }
                }

                if in_spine && local == "itemref" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"idref" {
                            spine.push(String::from_utf8_lossy(&attr.value).to_string());
                        }
                    }
                }
            }
            buf.clear();
        }

        if let Some(ref cid) = cover_id {
            if let Some(item) = manifest.get(cid) {
                metadata.cover_href = Some(item.href.clone());
            }
        }

        Ok((metadata, manifest, spine))
    }

    fn load_toc<R: Read + std::io::Seek>(
        archive: &mut zip::ZipArchive<R>,
        opf_dir: &str,
        manifest: &HashMap<String, ManifestItem>,
    ) -> Result<Vec<TocEntry>> {
        let ncx_item = manifest.values().find(|item| {
            item.media_type == "application/x-dtbncx+xml"
        });

        if let Some(ncx) = ncx_item {
            let full_path = if opf_dir.is_empty() {
                ncx.href.clone()
            } else {
                format!("{}/{}", opf_dir, ncx.href)
            };
            if let Ok(ncx_xml) = Self::read_zip_entry(archive, &full_path) {
                return Self::parse_ncx(&ncx_xml);
            }
        }

        let nav_item = manifest.values().find(|item| {
            item.media_type == "application/xhtml+xml"
                && item.href.contains("nav")
        });

        if let Some(nav) = nav_item {
            let full_path = if opf_dir.is_empty() {
                nav.href.clone()
            } else {
                format!("{}/{}", opf_dir, nav.href)
            };
            if let Ok(nav_html) = Self::read_zip_entry(archive, &full_path) {
                return Self::parse_nav(&nav_html);
            }
        }

        Ok(Vec::new())
    }

    fn parse_ncx(xml: &str) -> Result<Vec<TocEntry>> {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();
        let mut toc = Vec::new();
        let mut current_title = String::new();
        let mut current_href = String::new();
        let mut in_text = false;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if tag == "text" {
                        in_text = true;
                    }
                    if tag == "content" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"src" {
                                current_href = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if in_text {
                        current_title = e.unescape().unwrap_or_default().to_string();
                    }
                }
                Ok(Event::End(ref e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if tag == "text" {
                        in_text = false;
                    }
                    if tag == "navPoint" {
                        if !current_title.is_empty() {
                            toc.push(TocEntry {
                                title: current_title.clone(),
                                href: current_href.clone(),
                                level: 0,
                            });
                        }
                        current_title.clear();
                        current_href.clear();
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow!("NCX parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(toc)
    }

    fn parse_nav(html: &str) -> Result<Vec<TocEntry>> {
        let mut reader = Reader::from_str(html);
        let mut buf = Vec::new();
        let mut toc = Vec::new();
        let mut in_nav = false;
        let mut in_a = false;
        let mut current_href = String::new();
        let mut current_title = String::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if tag == "nav" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"type" || attr.key.as_ref() == b"epub:type" {
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if val == "toc" {
                                    in_nav = true;
                                }
                            }
                        }
                    }
                    if in_nav && tag == "a" {
                        in_a = true;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"href" {
                                current_href = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if in_a {
                        current_title = e.unescape().unwrap_or_default().to_string();
                    }
                }
                Ok(Event::End(ref e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    if tag == "a" && in_a {
                        in_a = false;
                        if !current_title.is_empty() {
                            toc.push(TocEntry {
                                title: current_title.clone(),
                                href: current_href.clone(),
                                level: 0,
                            });
                        }
                        current_title.clear();
                        current_href.clear();
                    }
                    if tag == "nav" {
                        in_nav = false;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(anyhow!("Nav parse error: {}", e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(toc)
    }

    fn inline_resources(html: &str, resources: &HashMap<String, String>, _opf_dir: &str) -> String {
        use std::fmt::Write;

        // 按字符遍历（不是按字节），避免破坏 UTF-8 多字节序列（如中文）
        let mut result = String::with_capacity(html.len() * 2);
        let chars: Vec<char> = html.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // 检测 src=" 或 href="
            let is_src = i + 3 < chars.len() && chars[i] == 's' && chars[i+1] == 'r' && chars[i+2] == 'c' && chars[i+3] == '=';
            let is_href = i + 4 < chars.len() && chars[i] == 'h' && chars[i+1] == 'r' && chars[i+2] == 'e' && chars[i+3] == 'f' && chars[i+4] == '=';
            
            if is_src || is_href {
                let attr_name = if is_src { "src" } else { "href" };
                let attr_end = if is_src { i + 4 } else { i + 5 };
                // 跳过等号后的空白
                let mut p = attr_end;
                while p < chars.len() && (chars[p] == ' ' || chars[p] == '\t') { p += 1; }
                if p < chars.len() && (chars[p] == '"' || chars[p] == '\'') {
                    let quote = chars[p];
                    p += 1;
                    let val_start = p;
                    while p < chars.len() && chars[p] != quote { p += 1; }
                    if p < chars.len() {
                        let value: String = chars[val_start..p].iter().collect();
                        // 不处理 data: 和 http(s) URL
                        if !value.starts_with("data:") && !value.starts_with("http") {
                            // 查找匹配的资源（先精确 href，再 basename）
                            let data_uri = resources.get(&value).cloned()
                                .or_else(|| {
                                    Path::new(&value).file_name()
                                        .and_then(|n| n.to_str())
                                        .and_then(|bn| resources.get(bn).cloned())
                                });
                            if let Some(uri) = data_uri {
                                // 输出: src="data:..."
                                write!(&mut result, "{}=\"{}\"", attr_name, uri.replace('"', "&quot;")).unwrap();
                                p += 1; // 跳过闭引号
                                i = p;
                                continue;
                            }
                        }
                        // 不匹配则原样输出
                        let slice: String = chars[i..=p].iter().collect();
                        result.push_str(&slice);
                        p += 1;
                        i = p;
                        continue;
                    }
                }
            }
            // 复制当前字符（保留 UTF-8 编码）
            result.push(chars[i]);
            i += 1;
        }

        result
    }

    pub fn extract_cover(path: &Path) -> Result<Option<String>> {
        let book = Self::load_index(path)?;
        Ok(book.cover_data_uri.clone())
    }

    pub fn chapter_count(path: &Path) -> Result<usize> {
        let book = Self::load_index(path)?;
        Ok(book.spine.len())
    }

    /// 加载或获取缓存的 BookIndex（不预加载资源，速度快）
    pub fn load_index(path: &Path) -> Result<Arc<BookIndex>> {
        let cache_key = path.to_string_lossy().to_string();

        {
            let cache = BOOK_CACHE.lock().unwrap();
            if let Some(book) = cache.get(&cache_key) {
                return Ok(Arc::clone(book));
            }
        }

        let file = std::fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let container_xml = Self::read_zip_entry(&mut archive, "META-INF/container.xml")?;
        let opf_path = Self::parse_container(&container_xml)?;
        let opf_content = Self::read_zip_entry(&mut archive, &opf_path)?;
        let opf_dir = Path::new(&opf_path)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_string_lossy()
            .to_string();

        let (metadata, manifest, spine) = Self::parse_opf(&opf_content)?;

        let toc = Self::load_toc(&mut archive, &opf_dir, &manifest).unwrap_or_default();

        // 提取封面（只这一次）
        let cover_href = metadata.cover_href.clone().or_else(|| {
            manifest.values().find_map(|item| {
                if item.id.contains("cover") && item.media_type.starts_with("image/") {
                    Some(item.href.clone())
                } else {
                    None
                }
            })
        });

        let cover_data_uri = cover_href.as_ref().and_then(|href| {
            let full_path = if opf_dir.is_empty() {
                href.clone()
            } else {
                format!("{}/{}", opf_dir, href)
            };
            Self::read_zip_entry_bytes(&mut archive, &full_path).ok().map(|data| {
                let ext = Path::new(href)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("jpeg");
                let mime = match ext {
                    "png" => "image/png",
                    "webp" => "image/webp",
                    "gif" => "image/gif",
                    _ => "image/jpeg",
                };
                let b64 = BASE64.encode(&data);
                format!("data:{};base64,{}", mime, b64)
            })
        });

        // 估算每章字符数：从 zip 条目的未压缩大小推算
        // HTML 文本占比约 0.7（去掉标签后约 0.5-0.8），保守取 0.6
        let mut estimated_chars = Vec::with_capacity(spine.len());
        for itemref in &spine {
            if let Some(item) = manifest.get(itemref) {
                let full_path = if opf_dir.is_empty() {
                    item.href.clone()
                } else {
                    format!("{}/{}", opf_dir, item.href)
                };
                // 读取 zip 条目的未压缩大小（不读取内容）
                let raw_size = archive.by_name(&full_path)
                    .map(|f| f.size() as usize)
                    .unwrap_or(0);
                // HTML 文本占比约 0.6 倍
                let chars = std::cmp::max(100, (raw_size as f64 * 0.6) as usize);
                estimated_chars.push(chars);
            } else {
                estimated_chars.push(1000); // fallback
            }
        }

        let book = Arc::new(BookIndex {
            metadata,
            spine,
            manifest,
            toc,
            opf_dir,
            cover_data_uri,
            estimated_chars,
        });

        let mut cache = BOOK_CACHE.lock().unwrap();
        cache.insert(cache_key, Arc::clone(&book));
        Ok(book)
    }

    /// 绔嬪嵆鏃犳晥鍖栨煇涓€鏂囦欢鐨勪功绫嶇紦瀛樺拰绔犺妭缂撳瓨銆?
    pub fn invalidate(path: &Path) {
        let cache_key = path.to_string_lossy().to_string();
        BOOK_CACHE.lock().unwrap().remove(&cache_key);
        let mut chapters = CHAPTER_CACHE.lock().unwrap();
        chapters.map.retain(|(book_path, _), _| book_path != &cache_key);
        chapters.order.retain(|(book_path, _)| book_path != &cache_key);
    }

    /// 加载单个章节，带进度回调
    pub fn get_chapter_with_progress<F>(
        path: &Path,
        chapter_index: usize,
        on_progress: F,
    ) -> Result<ChapterInfo>
    where
        F: Fn(usize, usize, &str),
    {
        let book = Self::load_index(path)?;
        Self::get_chapter_from_index_with_progress(path, &book, chapter_index, on_progress)
    }

    fn get_chapter_from_index_with_progress<F>(
        path: &Path,
        book: &BookIndex,
        chapter_index: usize,
        on_progress: F,
    ) -> Result<ChapterInfo>
    where
        F: Fn(usize, usize, &str),
    {
        if chapter_index >= book.spine.len() {
            return Err(anyhow!("Chapter index out of bounds: {}", chapter_index));
        }

        let cache_key = (path.to_string_lossy().to_string(), chapter_index);
        if let Some(hit) = CHAPTER_CACHE.lock().unwrap().get(&cache_key) {
            on_progress(3, 3, "已缓存");
            return Ok((*hit).clone());
        }

        let itemref = &book.spine[chapter_index];
        let item = book.manifest.get(itemref)
            .ok_or_else(|| anyhow!("Manifest item not found: {}", itemref))?;

        on_progress(1, 3, "打开 ZIP");

        let file = std::fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        on_progress(2, 3, "提取章节内容");
        let full_path = if book.opf_dir.is_empty() {
            item.href.clone()
        } else {
            format!("{}/{}", book.opf_dir, item.href)
        };
        let raw_html = Self::read_zip_entry(&mut archive, &full_path)?;

        on_progress(3, 3, "内联资源");

        // 只内联当前章节中实际引用的资源
        let referenced = Self::find_referenced_resources(&raw_html);
        let resources = Self::load_referenced_resources(&mut archive, book, &referenced);

        let chapter_title = book.toc
            .iter()
            .find(|t| {
                let clean_href = t.href.split('#').next().unwrap_or(&t.href);
                clean_href == item.href || clean_href.ends_with(&item.href)
            })
            .map(|t| t.title.clone())
            .unwrap_or_else(|| format!("第 {} 章", chapter_index + 1));

        let content = Self::inline_resources(&raw_html, &resources, &book.opf_dir);

        let info = ChapterInfo {
            index: chapter_index,
            title: chapter_title,
            href: item.href.clone(),
            content,
        };

        CHAPTER_CACHE
            .lock()
            .unwrap()
            .insert(cache_key, Arc::new(info.clone()));

        Ok(info)
    }

    /// 扫描 HTML 找出实际引用的资源
    fn find_referenced_resources(html: &str) -> std::collections::HashSet<String> {
        let mut refs = std::collections::HashSet::new();
        // 简单的 src="..." 和 href="..." 提取（针对图片和 CSS）
        let bytes = html.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b's' && i + 4 < bytes.len() && &bytes[i..i+4] == b"src=" {
                i += 4;
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let quote = bytes[i];
                    i += 1;
                    let start = i;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    if let Ok(s) = std::str::from_utf8(&bytes[start..i]) {
                        if !s.starts_with("data:") && !s.starts_with("http") {
                            refs.insert(s.to_string());
                        }
                    }
                }
            } else if bytes[i] == b'h' && i + 5 < bytes.len() && &bytes[i..i+5] == b"href=" {
                i += 5;
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let quote = bytes[i];
                    i += 1;
                    let start = i;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    if let Ok(s) = std::str::from_utf8(&bytes[start..i]) {
                        if !s.starts_with("data:") && !s.starts_with("http") && s.ends_with(".css") {
                            refs.insert(s.to_string());
                        }
                    }
                }
            } else {
                i += 1;
            }
        }
        refs
    }

    /// 只加载引用的资源
    /// 图：base64 内联，单张上限 10MB
    /// 非图（CSS/字体等）：单文件 256KB，单章总额 2MB
    fn load_referenced_resources<R: Read + std::io::Seek>(
        archive: &mut zip::ZipArchive<R>,
        book: &BookIndex,
        refs: &std::collections::HashSet<String>,
    ) -> HashMap<String, String> {
        const NON_IMAGE_MAX_SINGLE: usize = 256 * 1024;
        const NON_IMAGE_MAX_TOTAL: usize = 2 * 1024 * 1024;
        const IMAGE_MAX_SINGLE: usize = 10 * 1024 * 1024; // 10MB per image

        let mut resources = HashMap::new();
        let mut non_image_loaded = 0usize;

        for href in refs {
            // 找到对应的 manifest item
            let mut found = None;
            for item in book.manifest.values() {
                if &item.href == href {
                    found = Some(item);
                    break;
                }
                if let Some(basename) = Path::new(href).file_name().and_then(|n| n.to_str()) {
                    if item.href.ends_with(basename) {
                        found = Some(item);
                        break;
                    }
                }
            }
            if let Some(item) = found {
                let full_path = if book.opf_dir.is_empty() {
                    item.href.clone()
                } else {
                    format!("{}/{}", book.opf_dir, item.href)
                };
                let is_image = item.media_type.starts_with("image/");

                if let Ok(data) = Self::read_zip_entry_bytes(archive, &full_path) {
                    if is_image {
                        if data.len() > IMAGE_MAX_SINGLE {
                            continue; // 跳过超过 10MB 的图
                        }
                    } else {
                        if non_image_loaded >= NON_IMAGE_MAX_TOTAL {
                            continue;
                        }
                        if data.len() > NON_IMAGE_MAX_SINGLE {
                            continue;
                        }
                        non_image_loaded += data.len();
                    }
                    let b64 = BASE64.encode(&data);
                    let data_uri = format!("data:{};base64,{}", item.media_type, b64);
                    resources.insert(item.href.clone(), data_uri.clone());
                    if let Some(basename) = Path::new(href).file_name().and_then(|n| n.to_str()) {
                        resources.insert(basename.to_string(), data_uri);
                    }
                }
            }
        }

        resources
    }

    /// 返回每章的估算字符数（使用 zip entry size × 0.6，无需读取内容，< 1ms）
    pub fn chapter_text_lengths(path: &Path) -> Result<Vec<usize>> {
        let book = Self::load_index(path)?;
        Ok(book.estimated_chars.clone())
    }
}

#[derive(Debug, Clone)]
pub struct ManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_container_rootfile() {
        let xml = r#"
            <container>
              <rootfiles>
                <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
              </rootfiles>
            </container>
        "#;
        assert_eq!(EpubParser::parse_container(xml).unwrap(), "OEBPS/content.opf");
    }

    #[test]
    fn parses_ncx_toc() {
        let xml = r#"
            <ncx>
              <navMap>
                <navPoint>
                  <navLabel><text>第一章</text></navLabel>
                  <content src="chapter1.xhtml"/>
                </navPoint>
              </navMap>
            </ncx>
        "#;
        let toc = EpubParser::parse_ncx(xml).unwrap();
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "第一章");
        assert_eq!(toc[0].href, "chapter1.xhtml");
    }

    #[test]
    fn parses_nav_toc() {
        let html = r#"
            <nav xmlns:epub="http://www.idpf.org/2007/ops" epub:type="toc">
              <ol>
                <li><a href="chapter1.xhtml">第一章</a></li>
              </ol>
            </nav>
        "#;
        let toc = EpubParser::parse_nav(html).unwrap();
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "第一章");
        assert_eq!(toc[0].href, "chapter1.xhtml");
    }
}
