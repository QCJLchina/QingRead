import { useEffect } from "react";
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { useSettingsStore } from "./store/settings";
import ErrorBoundary from "./components/ErrorBoundary";
import TopBar from "./components/TopBar";
import LibraryPage from "./pages/LibraryPage";
import ReaderPage from "./pages/ReaderPage";
import SettingsPage from "./pages/SettingsPage";

export default function App() {
  const { loadSettings, loaded } = useSettingsStore();

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  if (!loaded) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100vh",
          color: "var(--text-secondary)",
        }}
      >
        加载中...
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
                <div className="main-content">
                  <Routes>
                    <Route path="/" element={<LibraryPage />} />
                    <Route path="/settings" element={<SettingsPage />} />
                    <Route path="*" element={<Navigate to="/" replace />} />
                  </Routes>
                </div>
              </div>
            }
          />
        </Routes>
      </BrowserRouter>
    </ErrorBoundary>
  );
}
