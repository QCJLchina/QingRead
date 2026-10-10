import { useEffect } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import ErrorBoundary from "./components/ErrorBoundary";
import TopBar from "./components/TopBar";
import LibraryPage from "./pages/LibraryPage";
import ReaderPage from "./pages/ReaderPage";
import SettingsPage from "./pages/SettingsPage";
import { useSettingsStore } from "./store/settings";
import { listen } from "@tauri-apps/api/event";
import { useLibraryStore } from "./store/library";
import { invoke } from "@tauri-apps/api/core";
import { prepareToClose } from "./lib/close-preparation";

export default function App() {
  const { loadSettings, loaded } = useSettingsStore();

  useEffect(() => {
    const listener = listen("import-complete", () => {
      void useLibraryStore.getState().loadBooks();
    });
    return () => { void listener.then((unlisten) => unlisten()); };
  }, []);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    let busy = false;
    const listener = listen<boolean>("prepare-reader-close", async (event) => {
      if (busy) return;
      busy = true;
      try {
        await prepareToClose();
        await invoke("complete_reader_close", { quit: event.payload });
      } catch (error) {
        alert("保存阅读资料失败，窗口保持打开，请重试。\n" + String(error));
      } finally {
        busy = false;
      }
    });
    return () => { void listener.then((unlisten) => unlisten()); };
  }, []);

  if (!loaded) {
    return (
      <div className="app-boot">
        <div className="spinner" />
        <span>轻阅正在启动…</span>
      </div>
    );
  }

  return (
    <ErrorBoundary>
      <BrowserRouter>
        <Routes>
          <Route path="/reader/:bookId" element={<ReaderPage />} />
          <Route
            path="*"
            element={
              <div className="app-layout">
                <TopBar />
                <main className="main-content">
                  <Routes>
                    <Route path="/" element={<LibraryPage />} />
                    <Route path="/settings" element={<SettingsPage />} />
                    <Route path="*" element={<Navigate to="/" replace />} />
                  </Routes>
                </main>
              </div>
            }
          />
        </Routes>
      </BrowserRouter>
    </ErrorBoundary>
  );
}
