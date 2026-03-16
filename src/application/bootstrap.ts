import { readAdapterSnapshot } from "../adapter/runtime-adapter";
import type { ShellState } from "./types";

export async function bootstrapShell(): Promise<ShellState> {
  const snapshot = await readAdapterSnapshot();

  return {
    appTitle: "Remember",
    subtitle: "Phase 1 Task 3 - Command Envelope Shell",
    layers: {
      adapter: snapshot.adapter,
      application: "ready",
      repository: snapshot.repository,
    },
    runtimeStatus: snapshot.runtimeStatus,
    commandProbe: snapshot.commandProbe,
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
