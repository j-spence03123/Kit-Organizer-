// Thin, typed wrappers around Tauri IPC commands. All filesystem access
// happens in Rust; the frontend only sends paths chosen by the user.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CategoryConfig,
  CategoryRemap,
  Classification,
  AudioMeta,
  DashboardStats,
  DupDecision,
  ExecReport,
  ExportedConfig,
  FinishedEvent,
  ImportRecord,
  ImportStats,
  ImportSummary,
  LearnedRule,
  BulkResult,
  ReviewOverview,
  SimilarSamples,
  OpMode,
  OperationRow,
  OrganiseProgress,
  PlanPreview,
  SamplePage,
  SampleQuery,
  ScanProgress,
  Settings,
  UndoReport,
} from "../types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  getCategoryConfig: () => invoke<CategoryConfig>("get_category_config"),
  saveCategoryConfig: (config: CategoryConfig, remaps: CategoryRemap[] = []) =>
    invoke<void>("save_category_config", { config, remaps }),
  resetCategoryConfig: () => invoke<CategoryConfig>("reset_category_config"),
  categoryUsage: () => invoke<[string, number][]>("category_usage"),
  exportConfig: (path: string, includeSettings: boolean) =>
    invoke<void>("export_config", { path, includeSettings }),
  importConfig: (path: string) => invoke<ExportedConfig>("import_config", { path }),

  startScan: (sources: string[], outputDir: string | null, mode: OpMode) =>
    invoke<string>("start_scan", { sources, outputDir, mode }),
  cancelJob: (jobId: string) => invoke<boolean>("cancel_job", { jobId }),

  querySamples: (query: SampleQuery) => invoke<SamplePage>("query_samples", { query }),
  importSummary: (importId: string) => invoke<ImportSummary>("import_summary", { importId }),
  reassignSamples: (ids: string[], categoryId: string) =>
    invoke<number>("reassign_samples", { ids, categoryId }),
  reviewOverview: (importId: string) => invoke<ReviewOverview>("review_overview", { importId }),
  acceptMatching: (query: SampleQuery) => invoke<BulkResult>("accept_matching", { query }),
  reassignMatching: (query: SampleQuery, categoryId: string) => invoke<BulkResult>("reassign_matching", { query, categoryId }),
  exportReviewList: (importId: string, path: string) => invoke<number>("export_review_list", { importId, path }),
  similarSamples: (ids: string[]) => invoke<SimilarSamples | null>("similar_samples", { ids }),
  listLearnedRules: () => invoke<LearnedRule[]>("list_learned_rules"),
  deleteLearnedRule: (id: number) => invoke<void>("delete_learned_rule", { id }),
  clearLearnedRules: () => invoke<void>("clear_learned_rules"),
  markReviewed: (ids: string[], reviewed: boolean) => invoke<number>("mark_reviewed", { ids, reviewed }),
  setDuplicateDecision: (ids: string[], decision: DupDecision | null) =>
    invoke<number>("set_duplicate_decision", { ids, decision }),
  setSourcePack: (ids: string[], pack: string) => invoke<number>("set_source_pack", { ids, pack }),
  reclassifyImport: (importId: string) => invoke<number>("reclassify_import", { importId }),
  classifyPreview: (relativePath: string) => invoke<Classification>("classify_preview", { relativePath }),

  previewPlan: (importId: string, outputDir: string, mode: OpMode) =>
    invoke<PlanPreview>("preview_plan", { importId, outputDir, mode }),
  organise: (importId: string, outputDir: string, mode: OpMode, confirmMove: boolean) =>
    invoke<string>("organise", { importId, outputDir, mode, confirmMove }),
  undoImport: (importId: string) => invoke<UndoReport>("undo_import", { importId }),

  listImports: (limit?: number) => invoke<ImportRecord[]>("list_imports", { limit }),
  getImport: (importId: string) => invoke<ImportRecord | null>("get_import", { importId }),
  deleteImport: (importId: string) => invoke<void>("delete_import", { importId }),
  importOperations: (importId: string) => invoke<OperationRow[]>("import_operations", { importId }),
  dashboardStats: () => invoke<DashboardStats>("dashboard_stats"),
  libraryTree: () => invoke<[string, number][]>("library_tree"),
  sourcePacks: (importId?: string | null) => invoke<[string, number][]>("source_packs", { importId }),
  audioInfo: (path: string) => invoke<AudioMeta>("audio_info", { path }),
  recoveryMessages: () => invoke<string[]>("recovery_messages"),
};

export const events = {
  onScanProgress: (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
    listen<ScanProgress>("scan-progress", (e) => cb(e.payload)),
  onScanFinished: (cb: (p: FinishedEvent<ImportStats>) => void): Promise<UnlistenFn> =>
    listen<FinishedEvent<ImportStats>>("scan-finished", (e) => cb(e.payload)),
  onOrganiseProgress: (cb: (p: OrganiseProgress) => void): Promise<UnlistenFn> =>
    listen<OrganiseProgress>("organise-progress", (e) => cb(e.payload)),
  onOrganiseFinished: (cb: (p: FinishedEvent<ExecReport>) => void): Promise<UnlistenFn> =>
    listen<FinishedEvent<ExecReport>>("organise-finished", (e) => cb(e.payload)),
};

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
