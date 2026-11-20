// Global application state: navigation, active import, settings, category
// configuration and toasts. Kept deliberately small — page data is loaded
// by the pages themselves.
import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../services/api";
import type { CategoryConfig, Settings } from "../types";
import { flatten, type FlatCategory } from "../utils/categories";

export type Page = "dashboard" | "import" | "review" | "library" | "history" | "categories" | "settings";

export interface Toast {
  id: number;
  message: string;
  tone: "info" | "ok" | "error";
}

interface AppContextValue {
  page: Page;
  navigate: (p: Page) => void;
  activeImportId: string | null;
  setActiveImportId: (id: string | null) => void;
  settings: Settings | null;
  updateSettings: (s: Settings) => Promise<void>;
  config: CategoryConfig | null;
  categories: FlatCategory[];
  setConfig: (c: CategoryConfig) => void;
  reloadConfig: () => Promise<void>;
  toast: (message: string, tone?: Toast["tone"]) => void;
  toasts: Toast[];
  dismissToast: (id: number) => void;
  /** Bumped whenever library data changes so pages can refetch. */
  dataVersion: number;
  bumpData: () => void;
}

const AppContext = createContext<AppContextValue | null>(null);

let toastSeq = 0;

export function AppProvider({ children }: { children: ReactNode }) {
  const [page, setPage] = useState<Page>("dashboard");
  const [activeImportId, setActiveImportId] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [config, setConfig] = useState<CategoryConfig | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [dataVersion, setDataVersion] = useState(0);

  const toast = useCallback((message: string, tone: Toast["tone"] = "info") => {
    const id = ++toastSeq;
    setToasts((t) => [...t, { id, message, tone }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), tone === "error" ? 8000 : 4000);
  }, []);

  const reloadConfig = useCallback(async () => {
    setConfig(await api.getCategoryConfig());
  }, []);

  useEffect(() => {
    Promise.all([api.getSettings(), api.getCategoryConfig()])
      .then(([s, c]) => {
        setSettings(s);
        setConfig(c);
      })
      .catch((e) => toast(`Could not load settings: ${errorMessage(e)}`, "error"));
    api
      .recoveryMessages()
      .then((msgs) => msgs.forEach((m) => toast(m, "info")))
      .catch(() => undefined);
  }, [toast]);

  useEffect(() => {
    document.documentElement.dataset.accent = settings?.appearance.accent ?? "amber";
  }, [settings?.appearance.accent]);

  const updateSettings = useCallback(
    async (s: Settings) => {
      await api.saveSettings(s);
      setSettings(s);
    },
    [],
  );

  const categories = useMemo(() => (config ? flatten(config.categories) : []), [config]);

  const value: AppContextValue = {
    page,
    navigate: setPage,
    activeImportId,
    setActiveImportId,
    settings,
    updateSettings,
    config,
    categories,
    setConfig,
    reloadConfig,
    toast,
    toasts,
    dismissToast: (id) => setToasts((t) => t.filter((x) => x.id !== id)),
    dataVersion,
    bumpData: () => setDataVersion((v) => v + 1),
  };
  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used inside AppProvider");
  return ctx;
}
