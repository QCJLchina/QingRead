// 本地浏览器回归：模拟 Tauri IPC，不访问真实书库或 WebDAV。
const assert = require("node:assert/strict");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");

const fixture = () => {
  const data = { book_id: "book-1", status: "finished", bookmarks: [], history: [], updated_at: 1 };
  data.bookmarks.push({ id: "fallback", chapter_index: 0, anchor: { path: [999], offset: 0, snippet: "不存在的书签位置", ratio: 0.75 }, snippet: "不存在的书签位置", note: null, created_at: 0 });
  const callbacks = new Map(), listeners = new Map();
  let counter = 0;
  const paragraphs = (chapter) => Array.from({ length: 32 }, (_, i) =>
    "<p>" + chapter + "段落" + i + " " + "阅读内容用于验证分页定位与稳定恢复。".repeat(40) +
    ([20, 25].includes(i) ? " <b>alpha</b>beta&nbsp;needle " : "") + "</p>").join("");
  const books = [
    { id: "book-1", title: "回归阅读", author: "测试", cover: null, file_path: "fixture.epub", added_at: 1, file_size: 1, format: "epub" },
    { id: "book-2", title: "其他书籍", author: "测试", cover: null, file_path: "fixture.txt", added_at: 2, file_size: 1, format: "txt" },
  ];
  let progress = { book_id: "book-1", chapter_index: 0, page_in_chapter: 0, total_pages_read: 1, last_read: 10, anchor: null, mode: "paged" };
  const settings = { theme: "light", font_size: 18, line_height: 1.8, font_family: "sans-serif", custom_bg_image: null, data_dir: null, close_behavior: "quit", reading_mode: "paged", content_width: null, content_padding: 16 };
  const windowSettings = { layout: "standard", width: 940, height: 610, x: null, y: null, lock_ratio: false, always_on_top: false, low_distraction: false, hide_hotkey: "Ctrl+Alt+H", toolbar_auto_hide: false, blur_curtain: false };
  window.__fixture = { data, closed: false, progress: () => progress, errors: [] };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(_event, id) { listeners.delete(id); },
  };
  window.__emitFixture = (event, payload) => {
    for (const [id, listener] of listeners) {
      if (listener.event === event) callbacks.get(listener.handler)?.({ event, id, payload });
    }
  };
  const clone = (value) => JSON.parse(JSON.stringify(value));
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback(callback) { const id = ++counter; callbacks.set(id, callback); return id; },
    unregisterCallback(id) { callbacks.delete(id); },
    convertFileSrc(path) { return path; },
    async invoke(command, args = {}) {
      if (command === "plugin:event|listen") { const id = ++counter; listeners.set(id, args); return id; }
      if (command === "plugin:event|unlisten") { listeners.delete(args.eventId); return; }
      if (command === "get_settings") return clone(settings);
      if (command === "save_settings") { Object.assign(settings, args.settings); return; }
      if (command === "get_window_settings") return windowSettings;
      if (command === "list_books") return clone(books);
      if (command === "list_progress") return [clone(progress)];
      if (command === "load_progress") return clone(progress);
      if (command === "list_reading_summaries") return [{ book_id: data.book_id, status: data.status, bookmark_count: data.bookmarks.length, history_count: data.history.length, updated_at: data.updated_at }];
      if (command === "get_reading_data") { await new Promise(resolve => setTimeout(resolve, 150)); return clone(data); }
      if (command === "mark_reading_started") { if (data.status === "unread") data.status = "reading"; return clone(data); }
      if (command === "set_reading_status") { data.status = args.status; return clone(data); }
      if (command === "open_reader") return 3;
      if (command === "get_chapter_offsets") return [0, 1, 2, 3];
      if (command === "get_toc") return ["第一章", "第二章", "损坏章"].map((title, i) => ({ title, href: "", level: 0, chapter_index: i }));
      if (command === "load_chapter") {
        if (args.chapterIndex === 2) throw new Error("fixture chapter failure");
        return { index: args.chapterIndex, title: args.chapterIndex === 0 ? "第一章" : "第二章", content: paragraphs(args.chapterIndex) };
      }
      if (command === "save_progress") {
        if (window.__fixture.failWrites) throw new Error("fixture save failure");
        await new Promise(resolve => setTimeout(resolve, 30));
        progress = { book_id: args.bookId, chapter_index: args.chapterIndex, page_in_chapter: args.pageInChapter, anchor: args.anchor, last_read: 20, total_pages_read: args.totalPagesRead, mode: args.mode };
        return;
      }
      if (command === "append_history") {
        if (window.__fixture.failWrites) throw new Error("fixture save failure");
        await new Promise(resolve => setTimeout(resolve, 20));
        const entry = { id: String(++counter), kind: args.kind, chapter_index: args.chapterIndex, anchor: args.anchor, snippet: args.snippet, created_at: counter };
        const first = data.history[0];
        if (first && first.chapter_index === entry.chapter_index && JSON.stringify(first.anchor) === JSON.stringify(entry.anchor)) data.history[0] = entry;
        else data.history.unshift(entry);
        return clone(data);
      }
      if (command === "add_bookmark") {
        data.bookmarks.push({ id: String(++counter), chapter_index: args.chapterIndex, anchor: args.anchor, snippet: args.snippet, note: null, created_at: counter });
        return { data: clone(data), created: true };
      }
      if (command === "complete_reader_close") { window.__fixture.closed = true; return; }
      return null;
    },
  };
};

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.QA_BROWSER || "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe" });
  const page = await browser.newPage({ viewport: { width: 940, height: 610 } });
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.addInitScript(fixture);
  try {
    await page.goto(process.env.QA_URL || "http://127.0.0.1:1420/");
    await page.waitForLoadState("networkidle");
    await page.getByRole("button", { name: "继续阅读", exact: true }).waitFor();
    await page.getByPlaceholder("搜索书名或作者").fill("没有匹配");
    assert.equal(await page.getByRole("button", { name: "继续阅读", exact: true }).count(), 1);
    await page.getByRole("button", { name: "继续阅读", exact: true }).click();
    await page.waitForFunction(() => /第 \d+/.test(document.querySelector(".footer-position")?.textContent || ""));
    await page.waitForFunction(() => window.__fixture.data.history.length > 0);
    assert.equal(await page.evaluate(() => window.__fixture.data.status), "finished");

    await page.getByTitle("搜索", { exact: true }).click();
    await page.getByPlaceholder("在书中搜索...").fill("alphabeta");
    await page.locator(".search-panel").getByRole("button", { name: "搜索", exact: true }).click();
    await page.locator(".search-result-item").first().waitFor();
    assert.equal(await page.locator(".search-result-item").count(), 4);
    await page.locator(".search-result-item").first().click();
    await page.waitForFunction(() => Number(document.querySelector(".footer-position")?.textContent.match(/第 (\d+)/)?.[1]) > 1);
    await page.waitForFunction(() => CSS.highlights?.get("reader-search-match")?.size > 0);
    const visible = await page.evaluate(() => {
      const range = CSS.highlights.get("reader-search-match").values().next().value;
      const rect = range.getBoundingClientRect(), view = document.querySelector(".reader-content").getBoundingClientRect();
      return rect.left >= view.left && rect.left < view.right && rect.top < view.bottom;
    });
    assert.equal(visible, true, "same chapter search must reach the correct page");

    await page.getByRole("button", { name: "下一处", exact: true }).click();
    await page.waitForFunction(() => document.querySelector(".search-status")?.textContent.includes("第 2"));
    await page.locator(".search-result-item").filter({ has: page.locator(".result-chapter", { hasText: "第二章" }) }).first().click();
    await page.waitForFunction(() => document.querySelector(".toolbar-title")?.textContent.includes("第二章"));
    await page.waitForFunction(() => CSS.highlights?.get("reader-search-match")?.size > 0);
    await page.getByRole("button", { name: "关闭搜索", exact: true }).click();
    await page.getByTitle("添加书签", { exact: true }).click();
    await page.waitForFunction(() => window.__fixture.data.bookmarks.length === 2);
    await page.getByTitle("切换到滚动阅读", { exact: true }).click();
    await page.getByTitle("搜索", { exact: true }).click();
    await page.getByPlaceholder("在书中搜索...").fill("needle");
    await page.locator(".search-panel").getByRole("button", { name: "搜索", exact: true }).click();
    await page.locator(".search-result-item").first().waitFor();
    await page.locator(".search-result-item").first().click();
    await page.waitForFunction(() => document.querySelector(".reader-content")?.scrollTop > 100);
    await page.getByRole("button", { name: "关闭搜索", exact: true }).click();
    await page.getByTitle("切换到分页阅读", { exact: true }).click();
    await page.getByTitle("书签列表", { exact: true }).click();
    await page.getByRole("button", { name: /不存在的书签位置/ }).click();
    await page.waitForFunction(() => document.querySelector(".reader-toast")?.textContent.includes("已跳至附近位置"));
    assert.equal(await page.evaluate(() => {
      const match = document.querySelector(".footer-position").textContent.match(/第 (\d+) \/ (\d+)/);
      return Number(match[1]) >= Number(match[2]) * 0.65;
    }), true, "broken bookmark must fall back to chapter ratio");
    const count = await page.evaluate(() => window.__fixture.data.history.filter(h => h.chapter_index === 2).length);
    await page.getByTitle("目录", { exact: true }).click();
    await page.getByRole("button", { name: "损坏章", exact: true }).click();
    await page.locator(".reader-error").waitFor();
    assert.equal(await page.evaluate(() => window.__fixture.data.history.filter(h => h.chapter_index === 2).length), count);

    await page.setViewportSize({ width: 340, height: 230 });
    await page.getByLabel("更多阅读操作").waitFor();
    assert.equal(await page.evaluate(() => {
      const rect = document.querySelector(".reader-toolbar").getBoundingClientRect();
      return rect.right <= innerWidth && [...document.querySelectorAll(".reader-toolbar > .toolbar-group")].every(el => el.getBoundingClientRect().right <= innerWidth);
    }), true);
    await page.getByLabel("更多阅读操作").click();
    await page.getByRole("button", { name: "书签列表", exact: true }).waitFor({ state: "visible" });
    let saveFailureDialog = false;
    page.on("dialog", async dialog => { saveFailureDialog = dialog.message().includes("保存阅读资料失败"); await dialog.accept(); });
    await page.evaluate(() => { window.__fixture.failWrites = true; window.__emitFixture("prepare-reader-close", true); });
    await page.waitForFunction(() => !window.__fixture.closed);
    await page.waitForTimeout(100);
    assert.equal(saveFailureDialog, true);
    await page.evaluate(() => { window.__fixture.failWrites = false; });
    await page.evaluate(() => window.__emitFixture("prepare-reader-close", true));
    await page.waitForFunction(() => window.__fixture.closed);
    assert.equal(await page.evaluate(() => window.__fixture.data.history[0].kind), "leave");
    assert.equal(await page.evaluate(() => window.__fixture.progress().chapter_index), 0);
    assert.deepEqual(errors, []);
    console.log("PASS shelf resume, finished status, contiguous search, paged and scroll jumps, bookmark ratio fallback, failure history, narrow menu, close failure/retry/flush");
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
