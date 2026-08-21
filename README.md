# EpubReader v2

基于 Tauri 2 + React 18 + Rust 的轻量级电子书阅读器，支持 EPUB 和 TXT 格式。

---

## 特性

| 功能 | 说明 |
|------|------|
| 格式支持 | EPUB（含图片） / TXT（UTF-8 / UTF-16） |
| 阅读主题 | 明亮 / 暗黑 / 羊皮纸 / 护眼绿，一键切换 |
| 自定义背景 | 支持上传 JPG/PNG/WebP 作阅读底图 |
| 章节导航 | 侧边栏目录，点击跳任意章节 |
| 阅读进度 | 自动保存，重开书回到上次位置 |
| 全局搜索 | 跨章节模糊搜索（中英混合） |
| 系统托盘 | 最小化到托盘，后台运行（可选） |
| 数据目录 | 自定义书籍/进度/设置存放路径 |
| 拖入添加 | 书架页面直接拖入 EPUB/TXT 添加 |
| 命令行导入 | 支持 `epubreader.exe "book.epub"` 自动导入 |
| 鼠标滚轮 | 跨平台滚轮阅读 |
| 图片显示 | EPUB 内嵌图片 base64 自动加载，单张上限 10 MB |

---

## 安装与使用

### 系统要求

- Windows 10 22H2+ / Windows 11（已预装 WebView2 Runtime）
- 如提示缺少 WebView2：https://aka.ms/webview2

### 方式一：NSIS 安装器（推荐）

1. 双击 `EpubReader_2.0.0_x64-setup.exe`
2. 按向导安装
3. 从开始菜单或桌面快捷方式启动

安装目录默认 `C:\Program Files\EpubReader\`，数据存储在 `%APPDATA%\EpubReader\`。

### 方式二：绿色版

1. 将 `epubreader.exe` 放到任意目录
2. 双击运行，首次启动会在 `%APPDATA%\EpubReader\` 创建数据目录

---

## 使用方法

### 添加书籍

- **拖入**：在书架页面中央的虚线区域拖入 `.epub` 或 `.txt` 文件
- **浏览**：点击虚线区域中的「浏览文件」按钮选择文件
- **命令行**：`epubreader.exe "D:\books\novel.epub"`

### 阅读

- **翻章**：点击工具栏 ◀ ▶ 按钮，或点击左侧目录中的章节
- **滚轮**：鼠标滚轮上下翻页
- **搜索**：点击 🔍 按钮，输入关键词跨章节搜索
- **切换主题**：点击 🎨 按钮循环切换 4 种主题
- **返回书架**：点击 ← 书架 按钮或按 Esc

### 阅读进度

- 每一章的滚动位置会自动保存
- 重新打开书本会回到上次阅读位置
- 当前章节 + 整书页码显示在工具栏

### 设置

点击顶部导航栏「设置」进入：

| 设置项 | 说明 |
|--------|------|
| 主题 | 明亮 / 暗黑 / 羊皮纸 / 护眼绿 |
| 字号 | 12-32px 滑块调节 |
| 行高 | 1.2-3.0 滑块调节 |
| 字体 | 6 种预设字体 |
| 自定义背景 | 上传图片作为阅读背景 |
| 关闭行为 | 直接退出 或 最小化到托盘 |
| 数据目录 | 更改书籍/进度/设置存储位置 |

---

## 数据存储

```
默认数据目录：
├── books/           # 电子书源文件
├── covers/          # 封面图片缓存
├── progress/        # 阅读进度（每本书一个 JSON）
├── library.json     # 书架列表
└── settings.json    # 用户设置
```

- 默认：`%APPDATA%\EpubReader\`
- 可在「设置 → 数据目录」中切换到其他位置
- 迁移到新电脑：复制整个 `data/` 目录即可

---

## TXT 章节识别

解析器自动识别以下章节标记：

- **中文模式**：`第N章`、`第N回`、`第N节`、`第N卷`、`第N集`、`第N篇`
- **数字格式**：阿拉伯数字（123...）、中文数字（一二三...）
- **特殊标记**：序章、楔子、终章、尾声、后记、前言、引子、番外
- **英文模式**：`Chapter N`、`CHAPTER N`

如无章节标记，自动按约 5 万字符分章。

---

## 命令行

```powershell
# 启动并导入一本书
epubreader.exe "C:\path\to\book.epub"
epubreader.exe "D:\novels\book.txt"

# 同时导入多本
epubreader.exe "book1.epub" "book2.txt"
```

---

## 常见问题

**Q: 打开 exe 提示缺少 WebView2？**
A: Windows 10 1809 以下需手动安装：https://aka.ms/webview2

**Q: 拖入文件没反应？**
A: 需要拖到书架页面中央的虚线框区域上方，不要拖到其他位置。

**Q: EPUB 中文乱码？**
A: 本应用按 UTF-8 解析 EPUB 内容；如仍乱码请检查 EPUB 源文件编码。

**Q: TXT 章节识别不准？**
A: 确认章节标题以"第N章"等格式开头。不规范的标题会触发按字数自动分章。

**Q: 图片不显示？**
A: EPUB 图片通过受限本地资源协议按需读取；单个资源超过 64 MB 会被拒绝加载。

**Q: 数据怎么迁移？**
A: 复制 `data/` 目录到新电脑，在「设置 → 数据目录」指向该目录后重启应用。

---

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面框架 | Tauri 2.x |
| 后端语言 | Rust 1.75+ |
| 前端框架 | React 18 + TypeScript 5 |
| 构建工具 | Vite 5 |
| 状态管理 | Zustand |
| 路由 | React Router v6 |
| EPUB 解析 | `zip` + `quick-xml` + `ammonia` |
| TXT 解析 | 自实现（编码检测 + 智能分章） |
| 安装器 | NSIS |

---

## 开发

```powershell
# 安装依赖
npm install

# 开发模式（前端热重载）
npm run tauri dev

# 生产构建
npm run tauri build

# 产物位置
src-tauri\target\release\epubreader.exe                      # 绿色版
src-tauri\target\release\bundle\nsis\EpubReader_*.exe   # NSIS 安装器
```

注意：国内网络需配置 Rust crates 镜像，否则首次 `cargo build` 会卡住：
```toml
# ~/.cargo/config.toml
[source.crates-io]
replace-with = "ustc"
[source.ustc]
registry = "sparse+https://mirrors.ustc.edu.cn/crates.io-index/"
```

---

## 许可

仅供个人使用。
