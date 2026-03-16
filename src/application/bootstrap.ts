import { readAdapterSnapshot } from "../adapter/runtime-adapter";
import type { CommitItem, SeriesSummary, ShellState } from "./types";

export async function bootstrapShell(): Promise<ShellState> {
  const snapshot = await readAdapterSnapshot();

  return {
    appTitle: "Remember",
    subtitle: "Phase 1 Task 5 - Shared DTO Contracts",
    layers: {
      adapter: snapshot.adapter,
      application: "ready",
      repository: snapshot.repository,
    },
    runtimeStatus: snapshot.runtimeStatus,
    commandProbe: snapshot.commandProbe,
    seriesPreview: buildSeriesPreview(),
    timelinePreview: buildTimelinePreview(),
  };
}

function buildSeriesPreview(): SeriesSummary[] {
  return [
    {
      id: "series-inbox",
      name: "Inbox",
      status: "active",
      lastUpdatedAt: "2026-03-16T00:00:00Z",
      latestExcerpt: "first-note",
      createdAt: "2026-03-15T00:00:00Z",
    },
    {
      id: "series-project-a",
      name: "Project-A",
      status: "silent",
      lastUpdatedAt: "2026-03-08T00:00:00Z",
      latestExcerpt: "follow-up-note",
      createdAt: "2026-03-01T00:00:00Z",
    },
  ];
}

function buildTimelinePreview(): CommitItem[] {
  return [
    {
      id: "commit-preview-001",
      seriesId: "series-inbox",
      createdAt: "2026-03-15T15:32:00Z",
      content: "first-note",
    },
    {
      id: "commit-preview-002",
      seriesId: "series-inbox",
      createdAt: "2026-03-15T15:35:00Z",
      content: "follow-up-note",
    },
  ];
}
