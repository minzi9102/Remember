export type LayerState = "ready" | "stubbed";
export type RuntimeMode = "sqlite_only" | "postgres_only" | "dual_sync";
export type RuntimeSource = "native" | "mock";
export type SeriesStatus = "active" | "silent" | "archived";
export type ShellView = "series_list" | "timeline";
export type TimelineLoadState = "idle" | "loading" | "ready" | "error";
export type ShellInteractionMode = "browse" | "search" | "create_series" | "draft_commit";
export type PendingShellAction = "search" | "create_series" | "append_commit" | "archive_series";

export interface LayerHealth {
  adapter: LayerState;
  application: LayerState;
  repository: LayerState;
}

export interface RuntimeStatus {
  mode: RuntimeMode;
  usedFallback: boolean;
  warnings: string[];
  source: RuntimeSource;
}

export interface RpcError {
  code: string;
  message: string;
}

export interface StartupSelfHealSummary {
  scannedAlerts: number;
  repairedAlerts: number;
  unresolvedAlerts: number;
  failedAlerts: number;
  completedAt: string;
  messages: string[];
}

export interface RpcMeta {
  path: string;
  runtimeMode: RuntimeMode;
  usedFallback: boolean;
  respondedAtUnixMs: number;
  startupSelfHeal: StartupSelfHealSummary;
}

export interface SeriesSummary {
  id: string;
  name: string;
  status: SeriesStatus;
  lastUpdatedAt: string;
  latestExcerpt: string;
  createdAt: string;
  archivedAt?: string;
}

export interface CommitItem {
  id: string;
  seriesId: string;
  content: string;
  createdAt: string;
}

export interface SeriesCreateData {
  series: SeriesSummary;
}

export interface SeriesListData {
  items: SeriesSummary[];
  nextCursor: string | null;
  limitEcho: number;
}

export interface CommitAppendData {
  commit: CommitItem;
  series: SeriesSummary;
}

export interface TimelineListData {
  seriesId: string;
  items: CommitItem[];
  nextCursor: string | null;
}

export interface SeriesArchiveData {
  seriesId: string;
  archivedAt: string;
}

export interface SeriesScanSilentData {
  affectedSeriesIds: string[];
  thresholdDays: number;
}

export type RpcData =
  | SeriesCreateData
  | SeriesListData
  | CommitAppendData
  | TimelineListData
  | SeriesArchiveData
  | SeriesScanSilentData;

export interface RpcEnvelope<T = RpcData> {
  ok: boolean;
  data?: T;
  error?: RpcError;
  meta: RpcMeta;
}

export interface CommandProbe {
  source: RuntimeSource;
  path: string;
  envelope: RpcEnvelope;
}

export interface ShellState {
  appTitle: string;
  subtitle: string;
  layers: LayerHealth;
  runtimeStatus: RuntimeStatus;
  commandProbe: CommandProbe;
  view: ShellView;
  seriesList: SeriesSummary[];
  selectedSeriesId: string | null;
  activeTimelineSeries: SeriesSummary | null;
  timelineLoadState: TimelineLoadState;
  timelineItems: CommitItem[];
  navigationError: RpcError | null;
  interactionMode: ShellInteractionMode;
  searchQuery: string;
  newSeriesNameDraft: string;
  commitDraft: string;
  pendingAction: PendingShellAction | null;
  interactionFeedback: RpcError | null;
}
