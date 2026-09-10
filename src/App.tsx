import { useEffect } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import ErrorBoundary from "./components/ErrorBoundary";
import TopBar from "./components/TopBar";
import LibraryPage from "./pages/LibraryPage";
import ReaderPage from "./pages/ReaderPage";
import SettingsPage from "./pages/SettingsPage";
import { useSettingsStore } from "./store/settings";

export default function App() {
  const { loadSettings, loaded } = useSettingsStore();

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

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
