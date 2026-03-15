import { readAdapterSnapshot } from "../adapter/runtime-adapter";
import type { ShellState } from "./types";

export function bootstrapShell(): ShellState {
  const snapshot = readAdapterSnapshot();

  return {
    appTitle: "Remember",
    subtitle: "Phase 1 Task 1 - Layered Skeleton",
    layers: {
      adapter: snapshot.adapter,
      application: "ready",
      repository: snapshot.repository,
    },
    seriesPreview: [
      { id: "series-inbox", name: "Inbox", latestExcerpt: "first-note" },
      { id: "series-project-a", name: "Project-A", latestExcerpt: "follow-up-note" },
    ],
    timelinePreview: [
      { createdAt: "2026-03-15 15:32:00", content: "first-note" },
      { createdAt: "2026-03-15 15:35:00", content: "follow-up-note" },
    ],
  };
}
