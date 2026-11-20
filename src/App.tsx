import { AppProvider, useApp } from "./hooks/useApp";
import { AudioPlayerProvider } from "./hooks/useAudioPlayer";
import { Sidebar } from "./components/layout/Sidebar";
import { Toaster } from "./components/layout/Toaster";
import Dashboard from "./pages/Dashboard";
import ImportPage from "./pages/Import";
import Review from "./pages/Review";
import Library from "./pages/Library";
import History from "./pages/History";
import Categories from "./pages/Categories";
import SettingsPage from "./pages/Settings";

function Shell() {
  const { page, settings } = useApp();
  return (
    <AudioPlayerProvider volume={settings?.preview.volume ?? 0.8} loop={settings?.preview.loopPlayback ?? false}>
      <div className="flex h-full">
        <Sidebar />
        <main className="min-w-0 flex-1 overflow-hidden">
          {page === "dashboard" && <Dashboard />}
          {page === "import" && <ImportPage />}
          {page === "review" && <Review />}
          {page === "library" && <Library />}
          {page === "history" && <History />}
          {page === "categories" && <Categories />}
          {page === "settings" && <SettingsPage />}
        </main>
      </div>
      <Toaster />
    </AudioPlayerProvider>
  );
}

export default function App() {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}
