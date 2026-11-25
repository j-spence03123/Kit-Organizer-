// TypeScript mirrors of the Rust DTOs (serde camelCase).

export type FileKind = "audio" | "midi" | "preset" | "ignored" | "unsupported";
export type SampleType = "one-shot" | "loop" | "midi" | "preset";
export type OpMode = "copy" | "move";
export type DuplicateHandling = "skip" | "keep" | "ask";
export type DupDecision = "keep" | "skip";
export type LowConfidenceAction = "uncategorised" | "predicted" | "skip";
export type CollisionHandling = "rename" | "skip";
export type SampleStatus = "pending" | "organised" | "skipped" | "failed" | "undone";
export type ImportStatus =
  | "scanning"
  | "review"
  | "organising"
  | "completed"
  | "cancelled"
  | "interrupted"
  | "undone"
  | "partially-undone"
  | "failed";
export type KindHint = "any" | "oneShot" | "loop";

export interface FileTypeSettings {
  audio: string[];
  midi: string[];
  presets: string[];
}

export interface Settings {
  defaultOutputDir: string | null;
  defaultMode: OpMode;
  duplicateHandling: DuplicateHandling;
  confidenceThreshold: number;
  lowConfidenceAction: LowConfidenceAction;
  fileTypes: FileTypeSettings;
  collisionHandling: CollisionHandling;
  preview: { volume: number; autoplay: boolean; loopPlayback: boolean };
  appearance: { accent: string; compactRows: boolean };
  workerThreads: number;
  analyseAudio: boolean;
  learnFromCorrections: boolean;
  skipDawProjects: boolean;
}

export interface CategoryNode {
  id: string;
  name: string;
  aliases: string[];
  priority: number;
  kind: KindHint;
  loopTarget?: string | null;
  defaultChild?: string | null;
  children: CategoryNode[];
}

export interface ClassifierWeights {
  filename: number;
  phraseBonus: number;
  folderNear: number;
  folderDecay: number;
  rootFolderFactor: number;
  parentFactor: number;
  weakFactor: number;
  ancestorSupport: number;
  headNounBonus: number;
  priorityStep: number;
  saturation: number;
  loopThreshold: number;
  minScore: number;
  metadata: number;
  fuzzyFactor: number;
  learned: number;
  sibling: number;
  audio: number;
  keyLoop: number;
  keylessLoop: number;
  folderFloor: number;
}

export interface CategoryConfig {
  version: number;
  rootFolder: string;
  categories: CategoryNode[];
  weights: ClassifierWeights;
  genericFolderTerms: string[];
  loopTerms: string[];
  oneShotTerms: string[];
  midiCategory: string;
  presetCategory: string;
  uncategorisedCategory: string;
  vendorPrefixes: string[];
}

export interface AudioMeta {
  durationSecs: number | null;
  sampleRate: number | null;
  channels: number | null;
  bitDepth: number | null;
  codec: string | null;
  acidOneShot: boolean | null;
  acidTempo: number | null;
  smplLoopCount: number | null;
  rootNote: number | null;
  title: string | null;
  genre: string | null;
  comment: string | null;
  tagBpm: number | null;
  tagKey: string | null;
  features: AudioFeatures | null;
  error: string | null;
}

export interface AudioFeatures {
  analysedSecs: number;
  lowRatio: number;
  highRatio: number;
  zcr: number;
  attackMs: number;
  decayMs: number | null;
  onsets: number;
  periodicity: number;
  riseDb: number;
  tailSilent: boolean;
}

export interface Alternative {
  categoryId: string;
  path: string;
  score: number;
}

export interface Classification {
  categoryId: string;
  categoryPath: string;
  confidence: number;
  evidence: string[];
  alternatives: Alternative[];
  sampleType: SampleType;
  loopScore: number | null;
  bpm: number | null;
  bpmSource: string | null;
  key: string | null;
  keySource: string | null;
  method: string;
}

export interface SampleRow {
  id: string;
  importId: string;
  originalPath: string;
  originalFilename: string;
  originalDir: string;
  relativeDir: string;
  sourcePack: string;
  size: number;
  hash: string | null;
  fileKind: FileKind;
  categoryId: string;
  categoryPath: string;
  confidence: number;
  evidence: string[];
  alternatives: Alternative[];
  sampleType: SampleType;
  bpm: number | null;
  bpmSource: string | null;
  key: string | null;
  keySource: string | null;
  method: string;
  audio: AudioMeta | null;
  manualOverride: boolean;
  reviewed: boolean;
  status: SampleStatus;
  duplicateOf: string | null;
  duplicatePath: string | null;
  dupDecision: DupDecision | null;
  destinationPath: string | null;
  error: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SampleQuery {
  importId?: string | null;
  statuses?: string[];
  search?: string | null;
  category?: string | null;
  minConfidence?: number | null;
  maxConfidence?: number | null;
  needsReviewBelow?: number | null;
  duplicatesOnly?: boolean;
  sourcePack?: string | null;
  relativeDir?: string | null;
  categoryId?: string | null;
  sort?: string | null;
  descending?: boolean;
  offset?: number;
  limit?: number;
}

export interface SamplePage {
  rows: SampleRow[];
  total: number;
}

export interface ImportStats {
  filesDiscovered: number;
  supported: number;
  unsupported: number;
  ignored: number;
  scanErrors: number;
  processed: number;
  copied: number;
  moved: number;
  skipped: number;
  duplicates: number;
  renamed: number;
  errors: number;
  reverted: number;
}

export interface ImportRecord {
  id: string;
  createdAt: string;
  completedAt: string | null;
  sources: string[];
  outputDir: string;
  libraryRoot: string;
  mode: OpMode;
  status: ImportStatus;
  stats: ImportStats;
  messages: string[];
}

export interface ImportSummary {
  total: number;
  pending: number;
  needsReview: number;
  duplicates: number;
  duplicatesUndecided: number;
  organised: number;
  skipped: number;
  failed: number;
  byCategory: [string, number][];
}

export interface DashboardStats {
  totalImported: number;
  organised: number;
  awaitingReview: number;
  duplicates: number;
  imports: number;
  lastOutputDir: string | null;
}

export interface PlannedOp {
  sampleId: string;
  src: string;
  destDir: string;
  destName: string;
  destPath: string;
  action: "copy" | "move" | "skip";
  categoryId: string;
  categoryPath: string;
  hash: string | null;
  size: number;
  reason: string | null;
  renamedFrom: string | null;
  duplicate: boolean;
  skipStatus: SampleStatus | null;
}

export interface PlanSummary {
  transfers: number;
  bytes: number;
  skippedDuplicates: number;
  undecidedDuplicates: number;
  keptDuplicates: number;
  lowConfidenceRerouted: number;
  lowConfidenceSkipped: number;
  renamed: number;
  collisionsSkipped: number;
  missing: number;
  folders: number;
}

export interface PlanPreview {
  libraryRoot: string;
  mode: OpMode;
  summary: PlanSummary;
  transfers: PlannedOp[];
  renamed: PlannedOp[];
  skipped: PlannedOp[];
  rerouted: PlannedOp[];
  availableBytes: number | null;
}

export interface FsIssue {
  path: string;
  message: string;
}

export interface ExecReport {
  copied: number;
  moved: number;
  skipped: number;
  failed: number;
  renamed: number;
  cancelled: boolean;
  errors: FsIssue[];
  warnings: string[];
  libraryRoot: string;
}

export interface UndoReport {
  reverted: number;
  skipped: FsIssue[];
}

export interface OperationRow {
  id: number;
  importId: string;
  sampleId: string;
  op: string;
  src: string;
  dest: string;
  temp: string | null;
  hash: string | null;
  createdDirs: string[];
  state: string;
  error: string | null;
}

export interface ScanProgress {
  jobId: string;
  phase: "scanning" | "analysing";
  discovered: number;
  supported: number;
  unsupported: number;
  processed: number;
  total: number;
  current: string;
}

export interface OrganiseProgress {
  jobId: string;
  processed: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
  current: string;
}

export interface FinishedEvent<R = unknown> {
  jobId: string;
  ok: boolean;
  cancelled: boolean;
  error: string | null;
  report: R | null;
}

export interface ExportedConfig {
  app: string;
  formatVersion: number;
  categories: CategoryConfig;
  settings?: Settings;
}

export interface LearnedRule {
  id: number;
  kind: "name" | "folder";
  tokens: string[];
  scope: string | null;
  categoryId: string;
  categoryPath: string | null;
  example: string;
  hits: number;
  createdAt: string;
  description: string;
}

export interface SimilarSamples {
  ids: string[];
  description: string;
  categoryId: string;
  categoryPath: string;
}

export interface ReviewGroup {
  folder: string;
  categoryId: string;
  categoryPath: string;
  count: number;
  avgConfidence: number;
  examples: string[];
  reason: string;
}

export interface ReviewOverview {
  total: number;
  uncategorised: number;
  nearThreshold: number;
  low: number;
  reasons: [string, number][];
  groups: ReviewGroup[];
}

export interface BulkResult {
  changed: number;
  resolvedAutomatically: number;
  remaining: number;
}

export interface CategoryRemap {
  fromId: string;
  toId: string;
}
