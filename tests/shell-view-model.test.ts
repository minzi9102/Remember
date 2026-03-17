import { describe, expect, it } from "vitest";

import { buildInitialShellState, shellReducer } from "../src/application/shell-view-model";
import type { RpcEnvelope, SeriesListData } from "../src/application/types";

function buildSeriesEnvelope(overrides?: Partial<RpcEnvelope<SeriesListData>>): RpcEnvelope<SeriesListData> {
  return {
    ok: true,
    data: {
      items: [
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
      ],
      nextCursor: null,
      limitEcho: 50,
    },
    meta: {
      path: "series.list",
      runtimeMode: "sqlite_only",
      usedFallback: false,
      respondedAtUnixMs: 123,
      startupSelfHeal: {
        scannedAlerts: 0,
        repairedAlerts: 0,
        unresolvedAlerts: 0,
        failedAlerts: 0,
        completedAt: "2026-03-17T00:00:00Z",
        messages: [],
      },
    },
    ...overrides,
  };
}

function buildSnapshot() {
  return {
    adapter: "ready" as const,
    repository: "stubbed" as const,
    runtimeStatus: {
      mode: "sqlite_only" as const,
      usedFallback: false,
      warnings: [],
      source: "mock" as const,
    },
    commandProbe: {
      source: "mock" as const,
      path: "series.create",
      envelope: {
        ok: true,
        data: {
          series: {
            id: "series-inbox",
            name: "Inbox",
            status: "active" as const,
            lastUpdatedAt: "2026-03-16T00:00:00Z",
            latestExcerpt: "first-note",
            createdAt: "2026-03-15T00:00:00Z",
          },
        },
        meta: {
          path: "series.create",
          runtimeMode: "sqlite_only" as const,
          usedFallback: false,
          respondedAtUnixMs: 123,
          startupSelfHeal: {
            scannedAlerts: 0,
            repairedAlerts: 0,
            unresolvedAlerts: 0,
            failedAlerts: 0,
            completedAt: "2026-03-17T00:00:00Z",
            messages: [],
          },
        },
      },
    },
  };
}

describe("shell view model", () => {
  it("selects the first series when bootstrapped with data", () => {
    const state = buildInitialShellState(buildSnapshot(), buildSeriesEnvelope());

    expect(state.view).toBe("series_list");
    expect(state.selectedSeriesId).toBe("series-inbox");
    expect(state.timelineLoadState).toBe("idle");
    expect(state.interactionMode).toBe("browse");
    expect(state.pendingAction).toBeNull();
  });

  it("keeps an empty selection when the list is empty", () => {
    const state = buildInitialShellState(
      buildSnapshot(),
      buildSeriesEnvelope({
        data: {
          items: [],
          nextCursor: null,
          limitEcho: 50,
        },
      }),
    );

    expect(state.selectedSeriesId).toBeNull();
    expect(state.seriesList).toEqual([]);
  });

  it("opens timeline and returns to the list without losing selection", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesEnvelope());
    const selected = shellReducer(initial, {
      type: "series.selected",
      seriesId: "series-project-a",
    });
    const opening = shellReducer(selected, {
      type: "timeline.requested",
      seriesId: "series-project-a",
    });
    const loaded = shellReducer(opening, {
      type: "timeline.loaded",
      seriesId: "series-project-a",
      items: [
        {
          id: "commit-1",
          seriesId: "series-project-a",
          content: "follow-up-note",
          createdAt: "2026-03-08T09:00:00Z",
        },
      ],
    });
    const closed = shellReducer(loaded, { type: "timeline.closed" });

    expect(loaded.view).toBe("timeline");
    expect(loaded.timelineLoadState).toBe("ready");
    expect(closed.view).toBe("series_list");
    expect(closed.selectedSeriesId).toBe("series-project-a");
  });

  it("moves selection through the list and clamps at the boundaries", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesEnvelope());
    const movedDown = shellReducer(initial, {
      type: "series.selection.moved",
      direction: "down",
    });
    const movedPastEnd = shellReducer(movedDown, {
      type: "series.selection.moved",
      direction: "down",
    });
    const movedUp = shellReducer(movedPastEnd, {
      type: "series.selection.moved",
      direction: "up",
    });

    expect(movedDown.selectedSeriesId).toBe("series-project-a");
    expect(movedPastEnd.selectedSeriesId).toBe("series-project-a");
    expect(movedUp.selectedSeriesId).toBe("series-inbox");
  });

  it("clears error state when retrying timeline load", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesEnvelope());
    const opening = shellReducer(initial, {
      type: "timeline.requested",
      seriesId: "series-inbox",
    });
    const failed = shellReducer(opening, {
      type: "timeline.failed",
      seriesId: "series-inbox",
      error: {
        code: "INVOKE_FAILED",
        message: "failed to load timeline",
      },
    });
    const retry = shellReducer(failed, {
      type: "timeline.requested",
      seriesId: "series-inbox",
    });

    expect(failed.timelineLoadState).toBe("error");
    expect(failed.navigationError?.code).toBe("INVOKE_FAILED");
    expect(retry.timelineLoadState).toBe("loading");
    expect(retry.navigationError).toBeNull();
  });

  it("falls back to the first series when the selected row disappears", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesEnvelope());
    const selected = shellReducer(initial, {
      type: "series.selected",
      seriesId: "series-project-a",
    });
    const replaced = shellReducer(selected, {
      type: "series.list.replaced",
      seriesList: [
        {
          id: "series-inbox",
          name: "Inbox",
          status: "active",
          lastUpdatedAt: "2026-03-16T00:00:00Z",
          latestExcerpt: "first-note",
          createdAt: "2026-03-15T00:00:00Z",
        },
      ],
      navigationError: null,
    });

    expect(replaced.selectedSeriesId).toBe("series-inbox");
  });

  it("opens and cancels input modes while clearing drafts", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesEnvelope());
    const search = shellReducer(initial, {
      type: "interaction.search.opened",
    });
    const searchChanged = shellReducer(search, {
      type: "interaction.search.changed",
      query: "Inbox",
    });
    const create = shellReducer(searchChanged, {
      type: "interaction.create_series.opened",
    });
    const createChanged = shellReducer(create, {
      type: "interaction.create_series.changed",
      value: "New Series",
    });
    const draft = shellReducer(createChanged, {
      type: "interaction.draft_commit.opened",
      initialContent: "f",
    });
    const cancelled = shellReducer(draft, {
      type: "interaction.cancelled",
    });

    expect(search.interactionMode).toBe("search");
    expect(searchChanged.searchQuery).toBe("Inbox");
    expect(create.interactionMode).toBe("create_series");
    expect(createChanged.newSeriesNameDraft).toBe("New Series");
    expect(draft.commitDraft).toBe("f");
    expect(cancelled.interactionMode).toBe("browse");
    expect(cancelled.searchQuery).toBe("");
    expect(cancelled.newSeriesNameDraft).toBe("");
    expect(cancelled.commitDraft).toBe("");
  });

  it("uses a preferred series id when refreshing the list", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesEnvelope());
    const replaced = shellReducer(initial, {
      type: "series.list.replaced",
      seriesList: buildSeriesEnvelope().data?.items ?? [],
      navigationError: null,
      preferredSeriesId: "series-project-a",
    });

    expect(replaced.selectedSeriesId).toBe("series-project-a");
  });
});
