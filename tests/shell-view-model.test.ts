import { describe, expect, it } from "vitest";

import { buildInitialShellState, shellReducer } from "../src/application/shell-view-model";
import type { SeriesSummary } from "../src/application/types";

function buildSeriesList(overrides?: SeriesSummary[]): SeriesSummary[] {
  return overrides ?? [
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
      path: "series.list",
      envelope: {
        ok: true,
        data: {
          items: [],
          nextCursor: null,
          limitEcho: 50,
        },
        meta: {
          path: "series.list",
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
    const state = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);

    expect(state.view).toBe("series_list");
    expect(state.seriesCollection).toBe("active");
    expect(state.selectedSeriesId).toBe("series-inbox");
    expect(state.activeSelectedSeriesId).toBe("series-inbox");
    expect(state.archivedSelectedSeriesId).toBeNull();
    expect(state.timelineLoadState).toBe("idle");
    expect(state.interactionMode).toBe("browse");
    expect(state.pendingAction).toBeNull();
  });

  it("keeps an empty selection when the list is empty", () => {
    const state = buildInitialShellState(buildSnapshot(), [], null);

    expect(state.selectedSeriesId).toBeNull();
    expect(state.seriesList).toEqual([]);
  });

  it("opens timeline and returns to the list without losing selection", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
    const selected = shellReducer(initial, {
      type: "series.selected",
      seriesId: "series-project-a",
    });
    const opening = shellReducer(selected, {
      type: "timeline.requested",
      seriesId: "series-project-a",
      presentation: "focus",
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
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
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
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
    const opening = shellReducer(initial, {
      type: "timeline.requested",
      seriesId: "series-inbox",
      presentation: "focus",
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
      presentation: "focus",
    });

    expect(failed.timelineLoadState).toBe("error");
    expect(failed.navigationError?.code).toBe("INVOKE_FAILED");
    expect(retry.timelineLoadState).toBe("loading");
    expect(retry.navigationError).toBeNull();
  });

  it("falls back to the first series when the selected row disappears", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
    const selected = shellReducer(initial, {
      type: "series.selected",
      seriesId: "series-project-a",
    });
    const replaced = shellReducer(selected, {
      type: "series.list.replaced",
      collection: "active",
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
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
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
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
    const replaced = shellReducer(initial, {
      type: "series.list.replaced",
      collection: "active",
      seriesList: buildSeriesList(),
      navigationError: null,
      preferredSeriesId: "series-project-a",
    });

    expect(replaced.selectedSeriesId).toBe("series-project-a");
  });

  it("keeps independent selections for active and archived collections", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
    const selectedActive = shellReducer(initial, {
      type: "series.selected",
      seriesId: "series-project-a",
    });
    const archivedList = shellReducer(selectedActive, {
      type: "series.list.replaced",
      collection: "archived",
      seriesList: [
        {
          id: "series-archive",
          name: "Archive",
          status: "archived",
          lastUpdatedAt: "2026-03-17T00:00:00Z",
          latestExcerpt: "frozen note",
          createdAt: "2026-03-15T00:00:00Z",
          archivedAt: "2026-03-17T00:00:00Z",
        },
      ],
      navigationError: null,
    });
    const backToActive = shellReducer(archivedList, {
      type: "series.list.replaced",
      collection: "active",
      seriesList: buildSeriesList(),
      navigationError: null,
    });

    expect(archivedList.seriesCollection).toBe("archived");
    expect(archivedList.selectedSeriesId).toBe("series-archive");
    expect(archivedList.activeSelectedSeriesId).toBe("series-project-a");
    expect(archivedList.archivedSelectedSeriesId).toBe("series-archive");
    expect(backToActive.seriesCollection).toBe("active");
    expect(backToActive.selectedSeriesId).toBe("series-project-a");
  });

  it("returns to the archived collection after closing an archived timeline", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
    const archivedList = shellReducer(initial, {
      type: "series.list.replaced",
      collection: "archived",
      seriesList: [
        {
          id: "series-archive",
          name: "Archive",
          status: "archived",
          lastUpdatedAt: "2026-03-17T00:00:00Z",
          latestExcerpt: "frozen note",
          createdAt: "2026-03-15T00:00:00Z",
          archivedAt: "2026-03-17T00:00:00Z",
        },
      ],
      navigationError: null,
    });
    const opening = shellReducer(archivedList, {
      type: "timeline.requested",
      seriesId: "series-archive",
      presentation: "focus",
    });
    const loaded = shellReducer(opening, {
      type: "timeline.loaded",
      seriesId: "series-archive",
      items: [
        {
          id: "commit-archive-1",
          seriesId: "series-archive",
          content: "frozen note",
          createdAt: "2026-03-16T00:00:00Z",
        },
      ],
    });
    const closed = shellReducer(loaded, { type: "timeline.closed" });

    expect(closed.seriesCollection).toBe("archived");
    expect(closed.selectedSeriesId).toBe("series-archive");
  });

  it("surfaces an initial interaction warning when silent scan fails", () => {
    const state = buildInitialShellState(
      buildSnapshot(),
      buildSeriesList(),
      null,
      {
        code: "INTERNAL_ERROR",
        message: "failed to refresh silent series status",
      },
    );

    expect(state.interactionFeedback).toEqual({
      code: "INTERNAL_ERROR",
      message: "failed to refresh silent series status",
    });
  });

  it("keeps list view while requesting timeline preview", () => {
    const initial = buildInitialShellState(buildSnapshot(), buildSeriesList(), null);
    const previewRequested = shellReducer(initial, {
      type: "timeline.requested",
      seriesId: "series-inbox",
      presentation: "preview",
    });
    const previewLoaded = shellReducer(previewRequested, {
      type: "timeline.loaded",
      seriesId: "series-inbox",
      items: [
        {
          id: "commit-1",
          seriesId: "series-inbox",
          content: "first-note",
          createdAt: "2026-03-16T00:00:00Z",
        },
      ],
    });

    expect(previewRequested.view).toBe("series_list");
    expect(previewRequested.timelineLoadState).toBe("loading");
    expect(previewLoaded.view).toBe("series_list");
    expect(previewLoaded.timelineLoadState).toBe("ready");
    expect(previewLoaded.activeTimelineSeries?.id).toBe("series-inbox");
  });
});
