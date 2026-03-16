export type LayerState = "ready" | "stubbed";
export type RuntimeMode = "sqlite_only" | "postgres_only" | "dual_sync";
export type RuntimeSource = "native" | "mock";

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

export interface RpcMeta {
  path: string;
  runtimeMode: RuntimeMode;
  usedFallback: boolean;
  respondedAtUnixMs: number;
}

export interface RpcEnvelope<T = Record<string, unknown>> {
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

export interface SeriesPreview {
  id: string;
  name: string;
  latestExcerpt: string;
}

export interface TimelinePreviewItem {
  createdAt: string;
  content: string;
}

export interface ShellState {
  appTitle: string;
  subtitle: string;
  layers: LayerHealth;
  runtimeStatus: RuntimeStatus;
  commandProbe: CommandProbe;
  seriesPreview: SeriesPreview[];
  timelinePreview: TimelinePreviewItem[];
}
