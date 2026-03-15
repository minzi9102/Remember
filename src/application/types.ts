export type LayerState = "ready" | "stubbed";

export interface LayerHealth {
  adapter: LayerState;
  application: LayerState;
  repository: LayerState;
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
  seriesPreview: SeriesPreview[];
  timelinePreview: TimelinePreviewItem[];
}
