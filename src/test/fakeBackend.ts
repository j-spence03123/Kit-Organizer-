// In-memory stand-in for the Rust backend used by UI tests.
import type {
  CategoryConfig,
  DashboardStats,
  ExecReport,
  FinishedEvent,
  ImportRecord,
  ImportStats,
  OrganiseProgress,
  PlanPreview,
  SampleRow,
  ScanProgress,
  SimilarSamples,
  BulkResult,
  ReviewOverview,
  Settings,
} from "../types";

type Listener<T> = (p: T) => void;

export const settings: Settings = {
  defaultOutputDir: null,
  defaultMode: "copy",
  duplicateHandling: "skip",
  confidenceThreshold: 0.6,
  lowConfidenceAction: "uncategorised",
  fileTypes: { audio: ["wav"], midi: ["mid"], presets: ["fxp"] },
  collisionHandling: "rename",
  preview: { volume: 0.8, autoplay: false, loopPlayback: false },
  appearance: { accent: "amber", compactRows: false },
  workerThreads: 0,
  analyseAudio: false,
  learnFromCorrections: true,
  skipDawProjects: true,
};

const node = (id: string, name: string, children: CategoryConfig["categories"] = []) => ({ id, name, aliases: [name.toLowerCase()], priority: 0, kind: "any" as const, children });

export const config: CategoryConfig = {
  version: 1,
  rootFolder: "Organised Samples",
  categories: [
    node("bass", "Bass", [node("bass/growls", "Growls"), node("bass/neuros", "Neuros")]),
    node("kicks", "Kicks"),
    node("snares", "Snares"),
    node("midi", "MIDI"),
    node("presets", "Presets"),
    node("uncategorised", "Uncategorised"),
  ],
  weights: {
    filename: 3, phraseBonus: 0.5, folderNear: 2, folderDecay: 0.75, rootFolderFactor: 0.4, parentFactor: 0.6,
    weakFactor: 0.5, ancestorSupport: 0.5, headNounBonus: 0.3, priorityStep: 0.25, saturation: 2, loopThreshold: 2,
    minScore: 0.75, metadata: 1.5, fuzzyFactor: 0.6, learned: 4, sibling: 2.5, audio: 1, keyLoop: 2.5, keylessLoop: 1.4, folderFloor: 0.75,
  },
  genericFolderTerms: ["samples"],
  loopTerms: ["loop"],
  oneShotTerms: ["one shot"],
  midiCategory: "midi",
  presetCategory: "presets",
  uncategorisedCategory: "uncategorised",
  vendorPrefixes: ["xlnt"],
};

function row(id: string, name: string, cat: string, conf: number): SampleRow {
  return {
    id, importId: "imp1", originalPath: `/packs/XLNT/${name}`, originalFilename: name, originalDir: "/packs/XLNT", relativeDir: "XLNT",
    sourcePack: "XLNT", size: 1000, hash: `h-${id}`, fileKind: "audio", categoryId: cat.toLowerCase(), categoryPath: cat, confidence: conf,
    evidence: [`Filename contains "${name}" → ${cat} (+3.00)`], alternatives: [], sampleType: "one-shot", bpm: null, bpmSource: null, key: null, keySource: null,
    method: "filename", audio: null, manualOverride: false, reviewed: false, status: "pending", duplicateOf: null, duplicatePath: null,
    dupDecision: null, destinationPath: null, error: null, createdAt: "", updatedAt: "",
  };
}

export function createBackend() {
  const listeners = {
    scanProgress: [] as Listener<ScanProgress>[],
    scanFinished: [] as Listener<FinishedEvent<ImportStats>>[],
    organiseProgress: [] as Listener<OrganiseProgress>[],
    organiseFinished: [] as Listener<FinishedEvent<ExecReport>>[],
  };
  const calls: { name: string; args: unknown[] }[] = [];
  let imports: ImportRecord[] = [];
  let samples: SampleRow[] = [];
  const stats = (): ImportStats => ({
    filesDiscovered: 4, supported: 3, unsupported: 1, ignored: 0, scanErrors: 0, processed: 0, copied: 0, moved: 0, skipped: 0, duplicates: 0, renamed: 0, errors: 0, reverted: 0,
  });

  const sub = <T,>(list: Listener<T>[], cb: Listener<T>) => {
    list.push(cb);
    return Promise.resolve(() => {
      const i = list.indexOf(cb);
      if (i >= 0) list.splice(i, 1);
    });
  };
  const log = (name: string, ...args: unknown[]) => calls.push({ name, args });
  const later = (fn: () => void) => setTimeout(fn, 5);

  const api = {
    getSettings: async () => settings,
    saveSettings: async () => undefined,
    getCategoryConfig: async () => config,
    saveCategoryConfig: async () => undefined,
    resetCategoryConfig: async () => config,
    categoryUsage: async () => [] as [string, number][],
    exportConfig: async () => undefined,
    importConfig: async () => ({ app: "kit-organizer", formatVersion: 1, categories: config }),
    recoveryMessages: async () => [] as string[],
    startScan: async (sources: string[], outputDir: string | null, mode: string) => {
      log("startScan", sources, outputDir, mode);
      const id = "imp1";
      imports = [{ id, createdAt: new Date().toISOString(), completedAt: null, sources, outputDir: outputDir ?? "", libraryRoot: "", mode: mode as "copy", status: "scanning", stats: stats(), messages: [] }];
      later(() => {
        listeners.scanProgress.forEach((l) => l({ jobId: id, phase: "analysing", discovered: 4, supported: 3, unsupported: 1, processed: 2, total: 3, current: "Kick_01.wav" }));
        samples = [row("s1", "XLNT_Growl_F.wav", "Bass/Growls", 0.83), row("s2", "Kick_01.wav", "Kicks", 0.78), row("s3", "Mystery_01.wav", "Uncategorised", 0)];
        imports[0]!.status = "review";
        listeners.scanFinished.forEach((l) => l({ jobId: id, ok: true, cancelled: false, error: null, report: stats() }));
      });
      return id;
    },
    cancelJob: async () => true,
    querySamples: async (q: { needsReviewBelow?: number | null; search?: string | null; statuses?: string[] }) => {
      let rows = samples;
      if (q.needsReviewBelow != null) rows = rows.filter((r) => r.confidence < q.needsReviewBelow! && !r.reviewed);
      if (q.search) rows = rows.filter((r) => r.originalFilename.toLowerCase().includes(q.search!.toLowerCase()));
      if (q.statuses?.length) rows = rows.filter((r) => q.statuses!.includes(r.status));
      return { rows, total: rows.length };
    },
    importSummary: async () => ({
      total: samples.length,
      pending: samples.filter((s) => s.status === "pending").length,
      needsReview: samples.filter((s) => s.confidence < 0.6 && !s.reviewed && s.status === "pending").length,
      duplicates: 0, duplicatesUndecided: 0,
      organised: samples.filter((s) => s.status === "organised").length,
      skipped: 0, failed: 0, byCategory: [],
    }),
    reassignSamples: async (ids: string[], categoryId: string) => {
      log("reassignSamples", ids, categoryId);
      samples = samples.map((s) => (ids.includes(s.id) ? { ...s, categoryId, categoryPath: categoryId === "kicks" ? "Kicks" : categoryId, manualOverride: true, reviewed: true } : s));
      return ids.length;
    },
    reviewOverview: async (): Promise<ReviewOverview> => ({
      total: 47,
      uncategorised: 1,
      nearThreshold: 30,
      low: 16,
      reasons: [["No recognisable keywords", 20], ["Only a broad category matched", 27]],
      groups: [
        { folder: "XLNT/Drums", categoryId: "snares", categoryPath: "Snares", count: 12, avgConfidence: 0.52, examples: ["Snr_01.wav"], reason: "Weak single keyword" },
        { folder: "XLNT", categoryId: "uncategorised", categoryPath: "Uncategorised", count: 1, avgConfidence: 0, examples: ["Mystery_01.wav"], reason: "No recognisable keywords" },
      ],
    }),
    acceptMatching: async (query: unknown): Promise<BulkResult> => {
      log("acceptMatching", query);
      return { changed: 12, resolvedAutomatically: 30, remaining: 5 };
    },
    reassignMatching: async (query: unknown, categoryId: string): Promise<BulkResult> => {
      log("reassignMatching", query, categoryId);
      return { changed: 1, resolvedAutomatically: 0, remaining: 46 };
    },
    exportReviewList: async () => 47,
    similarSamples: async (ids: string[]): Promise<SimilarSamples | null> => {
      log("similarSamples", ids);
      return null;
    },
    listLearnedRules: async () => [],
    deleteLearnedRule: async () => undefined,
    clearLearnedRules: async () => undefined,
    markReviewed: async (ids: string[]) => {
      samples = samples.map((s) => (ids.includes(s.id) ? { ...s, reviewed: true } : s));
      return ids.length;
    },
    setDuplicateDecision: async () => 0,
    setSourcePack: async () => 0,
    reclassifyImport: async () => 0,
    classifyPreview: async () => { throw new Error("not implemented"); },
    previewPlan: async (importId: string, outputDir: string, mode: string): Promise<PlanPreview> => {
      log("previewPlan", importId, outputDir, mode);
      const transfers = samples.map((s) => ({
        sampleId: s.id, src: s.originalPath, destDir: `${outputDir}/Organised Samples/${s.categoryPath}`, destName: s.originalFilename,
        destPath: `${outputDir}/Organised Samples/${s.categoryPath}/${s.originalFilename}`, action: mode as "copy", categoryId: s.categoryId, categoryPath: s.categoryPath,
        hash: s.hash, size: s.size, reason: null, renamedFrom: null, duplicate: false, skipStatus: null,
      }));
      return {
        libraryRoot: `${outputDir}/Organised Samples`, mode: mode as "copy",
        summary: { transfers: transfers.length, bytes: 3000, skippedDuplicates: 0, undecidedDuplicates: 0, keptDuplicates: 0, lowConfidenceRerouted: 0, lowConfidenceSkipped: 0, renamed: 0, collisionsSkipped: 0, missing: 0, folders: 3 },
        transfers, renamed: [], skipped: [], rerouted: [], availableBytes: 10_000_000,
      };
    },
    organise: async (importId: string, outputDir: string, mode: string, confirmMove: boolean) => {
      log("organise", importId, outputDir, mode, confirmMove);
      later(() => {
        listeners.organiseProgress.forEach((l) => l({ jobId: importId, processed: 1, total: 3, bytesDone: 1000, bytesTotal: 3000, current: "Kick_01.wav" }));
        samples = samples.map((s) => ({ ...s, status: "organised", destinationPath: `${outputDir}/Organised Samples/${s.categoryPath}/${s.originalFilename}` }));
        imports[0]!.status = "completed";
        imports[0]!.stats.copied = 3;
        listeners.organiseFinished.forEach((l) =>
          l({ jobId: importId, ok: true, cancelled: false, error: null, report: { copied: 3, moved: 0, skipped: 0, failed: 0, renamed: 0, cancelled: false, errors: [], warnings: [], libraryRoot: `${outputDir}/Organised Samples` } }),
        );
      });
      return importId;
    },
    undoImport: async () => ({ reverted: 3, skipped: [] }),
    listImports: async () => imports,
    getImport: async (id: string) => imports.find((i) => i.id === id) ?? null,
    deleteImport: async () => undefined,
    importOperations: async () => [],
    dashboardStats: async (): Promise<DashboardStats> => ({ totalImported: samples.length, organised: samples.filter((s) => s.status === "organised").length, awaitingReview: 0, duplicates: 0, imports: imports.length, lastOutputDir: imports[0]?.outputDir ?? null }),
    libraryTree: async () => {
      const m = new Map<string, number>();
      samples.filter((s) => s.status === "organised").forEach((s) => m.set(s.categoryPath, (m.get(s.categoryPath) ?? 0) + 1));
      return [...m];
    },
    sourcePacks: async () => [],
    audioInfo: async () => { throw new Error("no audio in tests"); },
  };

  const events = {
    onScanProgress: (cb: Listener<ScanProgress>) => sub(listeners.scanProgress, cb),
    onScanFinished: (cb: Listener<FinishedEvent<ImportStats>>) => sub(listeners.scanFinished, cb),
    onOrganiseProgress: (cb: Listener<OrganiseProgress>) => sub(listeners.organiseProgress, cb),
    onOrganiseFinished: (cb: Listener<FinishedEvent<ExecReport>>) => sub(listeners.organiseFinished, cb),
  };

  return { api, events, calls, samples: () => samples };
}
