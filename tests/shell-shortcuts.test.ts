import { describe, expect, it } from "vitest";

import { interpretShellKeyboardEvent } from "../src/application/shell-shortcuts";
import type { ShellState } from "../src/application/types";

function buildShell(overrides?: Partial<ShellState>): ShellState {
  return {
    appTitle: "Remember",
    subtitle: "Phase 4 Task 6 - Archived Read-only Timeline",
    layers: {
      adapter: "ready",
      application: "ready",
      repository: "stubbed",
    },
    runtimeStatus: {
      mode: "sqlite_only",
      usedFallback: false,
      warnings: [],
      source: "mock",
    },
    commandProbe: {
      source: "mock",
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
      },
    },
    view: "series_list",
    seriesCollection: "active",
    seriesList: [
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
    selectedSeriesId: "series-inbox",
    activeSelectedSeriesId: "series-inbox",
    archivedSelectedSeriesId: null,
    activeTimelineSeries: null,
    timelineLoadState: "idle",
    timelineItems: [],
    navigationError: null,
    interactionMode: "browse",
    searchQuery: "",
    newSeriesNameDraft: "",
    commitDraft: "",
    pendingAction: null,
    interactionFeedback: null,
    ...overrides,
  };
}

function buildKeyboardEvent(
  overrides?: Partial<Parameters<typeof interpretShellKeyboardEvent>[1]>,
) {
  return {
    key: "ArrowDown",
    shiftKey: false,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    repeat: false,
    isComposing: false,
    targetIsEditable: false,
    ...overrides,
  };
}

describe("shell keyboard shortcuts", () => {
  it("moves the list selection with arrow keys in browse mode", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell(),
      buildKeyboardEvent({ key: "ArrowDown" }),
    );

    expect(intent).toEqual({
      type: "move_selection",
      direction: "down",
    });
  });

  it("opens the timeline from the series list", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell(),
      buildKeyboardEvent({ key: "ArrowRight" }),
    );

    expect(intent).toEqual({
      type: "open_timeline",
    });
  });

  it("closes an input mode before changing views", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell({
        interactionMode: "search",
      }),
      buildKeyboardEvent({ key: "Escape", targetIsEditable: true }),
    );

    expect(intent).toEqual({
      type: "cancel_interaction",
    });
  });

  it("returns from the timeline with Escape", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell({
        view: "timeline",
      }),
      buildKeyboardEvent({ key: "Escape" }),
    );

    expect(intent).toEqual({
      type: "close_timeline",
    });
  });

  it("opens search with slash", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell(),
      buildKeyboardEvent({ key: "/" }),
    );

    expect(intent).toEqual({
      type: "open_search",
    });
  });

  it("opens create series with Shift+N", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell(),
      buildKeyboardEvent({ key: "N", shiftKey: true }),
    );

    expect(intent).toEqual({
      type: "open_create_series",
    });
  });

  it("allows search and timeline open in the archived collection", () => {
    expect(
      interpretShellKeyboardEvent(
        buildShell({
          seriesCollection: "archived",
          seriesList: [
            {
              id: "series-archive",
              name: "Archive",
              status: "archived",
              lastUpdatedAt: "2026-03-16T00:00:00Z",
              latestExcerpt: "frozen note",
              createdAt: "2026-03-15T00:00:00Z",
              archivedAt: "2026-03-17T00:00:00Z",
            },
          ],
          selectedSeriesId: "series-archive",
          archivedSelectedSeriesId: "series-archive",
        }),
        buildKeyboardEvent({ key: "/" }),
      ),
    ).toEqual({ type: "open_search" });

    expect(
      interpretShellKeyboardEvent(
        buildShell({
          seriesCollection: "archived",
          seriesList: [
            {
              id: "series-archive",
              name: "Archive",
              status: "archived",
              lastUpdatedAt: "2026-03-16T00:00:00Z",
              latestExcerpt: "frozen note",
              createdAt: "2026-03-15T00:00:00Z",
              archivedAt: "2026-03-17T00:00:00Z",
            },
          ],
          selectedSeriesId: "series-archive",
          archivedSelectedSeriesId: "series-archive",
        }),
        buildKeyboardEvent({ key: "ArrowRight" }),
      ),
    ).toEqual({ type: "open_timeline" });
  });

  it("submits create series from create mode", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell({
        interactionMode: "create_series",
      }),
      buildKeyboardEvent({ key: "Enter", targetIsEditable: true }),
    );

    expect(intent).toEqual({
      type: "submit_create_series",
    });
  });

  it("submits commit drafts from draft mode", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell({
        interactionMode: "draft_commit",
      }),
      buildKeyboardEvent({ key: "Enter", targetIsEditable: true }),
    );

    expect(intent).toEqual({
      type: "submit_commit",
    });
  });

  it("archives a selected silent series", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell({
        selectedSeriesId: "series-project-a",
      }),
      buildKeyboardEvent({ key: "a" }),
    );

    expect(intent).toEqual({
      type: "archive_selected",
    });
  });

  it("blocks archive on non-silent series", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell(),
      buildKeyboardEvent({ key: "a" }),
    );

    expect(intent).toEqual({
      type: "blocked",
      feedback: {
        code: "ARCHIVE_DISABLED",
        message: "only silent series can be archived with `a`",
      },
    });
  });

  it("blocks write shortcuts in the archived collection", () => {
    const shell = buildShell({
      seriesCollection: "archived",
      seriesList: [
        {
          id: "series-archive",
          name: "Archive",
          status: "archived",
          lastUpdatedAt: "2026-03-16T00:00:00Z",
          latestExcerpt: "frozen note",
          createdAt: "2026-03-15T00:00:00Z",
          archivedAt: "2026-03-17T00:00:00Z",
        },
      ],
      selectedSeriesId: "series-archive",
      archivedSelectedSeriesId: "series-archive",
    });

    expect(
      interpretShellKeyboardEvent(shell, buildKeyboardEvent({ key: "a" })),
    ).toEqual({
      type: "blocked",
      feedback: {
        code: "ARCHIVE_READ_ONLY",
        message: "archived series are read-only; switch to Active to make changes",
      },
    });

    expect(
      interpretShellKeyboardEvent(shell, buildKeyboardEvent({ key: "N", shiftKey: true })),
    ).toEqual({
      type: "blocked",
      feedback: {
        code: "ARCHIVE_READ_ONLY",
        message: "archived series are read-only; switch to Active to make changes",
      },
    });

    expect(
      interpretShellKeyboardEvent(shell, buildKeyboardEvent({ key: "x" })),
    ).toEqual({
      type: "blocked",
      feedback: {
        code: "ARCHIVE_READ_ONLY",
        message: "archived series are read-only; switch to Active to make changes",
      },
    });
  });

  it("starts a commit draft from the first printable key", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell(),
      buildKeyboardEvent({ key: "x" }),
    );

    expect(intent).toEqual({
      type: "start_commit_draft",
      initialContent: "x",
    });
  });

  it("ignores repeats, IME composition, modifiers, and editable targets", () => {
    expect(
      interpretShellKeyboardEvent(
        buildShell(),
        buildKeyboardEvent({ key: "x", repeat: true }),
      ),
    ).toEqual({ type: "noop" });

    expect(
      interpretShellKeyboardEvent(
        buildShell(),
        buildKeyboardEvent({ key: "x", isComposing: true }),
      ),
    ).toEqual({ type: "noop" });

    expect(
      interpretShellKeyboardEvent(
        buildShell(),
        buildKeyboardEvent({ key: "x", ctrlKey: true }),
      ),
    ).toEqual({ type: "noop" });

    expect(
      interpretShellKeyboardEvent(
        buildShell(),
        buildKeyboardEvent({ key: "x", targetIsEditable: true }),
      ),
    ).toEqual({ type: "noop" });
  });

  it("blocks printable shortcuts when no series is selected", () => {
    const intent = interpretShellKeyboardEvent(
      buildShell({
        selectedSeriesId: null,
      }),
      buildKeyboardEvent({ key: "x" }),
    );

    expect(intent).toEqual({
      type: "blocked",
      feedback: {
        code: "NO_SERIES_SELECTED",
        message: "select a series before writing a commit",
      },
    });
  });
});
