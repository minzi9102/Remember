import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ShellState } from "../src/application/types";
import { RememberShell } from "../src/ui/RememberShell";

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
      mode: "dual_sync",
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
          runtimeMode: "dual_sync",
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

const noop = () => undefined;

function renderShellMarkup(shell: ShellState) {
  return renderToStaticMarkup(
    <RememberShell
      shell={shell}
      onSelectCollection={noop}
      onSelectSeries={noop}
      onOpenTimeline={noop}
      onBackToList={noop}
      onRetryTimeline={noop}
      onSearchQueryChange={noop}
      onNewSeriesNameDraftChange={noop}
      onCommitDraftChange={noop}
    />,
  );
}

describe("RememberShell list and timeline views", () => {
  it("renders the series list state and diagnostics", () => {
    const markup = renderShellMarkup(buildShell());

    expect(markup).toContain("Series");
    expect(markup).toContain("Inbox");
    expect(markup).toContain("Silent");
    expect(markup).toContain("View timeline");
    expect(markup).toContain("Active");
    expect(markup).toContain("Archived");
    expect(markup).toContain("Startup Self-Heal");
    expect(markup).toContain("No unresolved startup alerts.");
  });

  it("renders unresolved startup self-heal messages", () => {
    const shell = buildShell({
      commandProbe: {
        source: "mock",
        path: "series.list",
        envelope: {
          ok: false,
          error: {
            code: "DUAL_WRITE_FAILED",
            message: "simulated",
          },
          meta: {
            path: "series.list",
            runtimeMode: "dual_sync",
            usedFallback: false,
            respondedAtUnixMs: 456,
            startupSelfHeal: {
              scannedAlerts: 2,
              repairedAlerts: 1,
              unresolvedAlerts: 1,
              failedAlerts: 1,
              completedAt: "2026-03-17T00:10:00Z",
              messages: ["alert `a` remains unresolved"],
            },
          },
        },
      },
    });

    const markup = renderShellMarkup(shell);

    expect(markup).toContain("unresolved: 1");
    expect(markup).toContain("failed: 1");
    expect(markup).toContain("Unresolved startup alerts");
    expect(markup).toContain("alert `a` remains unresolved");
  });

  it("renders the timeline state with read-only items", () => {
    const markup = renderShellMarkup(
      buildShell({
        view: "timeline",
        activeTimelineSeries: {
          id: "series-inbox",
          name: "Inbox",
          status: "active",
          lastUpdatedAt: "2026-03-16T00:00:00Z",
          latestExcerpt: "first-note",
          createdAt: "2026-03-15T00:00:00Z",
        },
        timelineLoadState: "ready",
        timelineItems: [
          {
            id: "commit-1",
            seriesId: "series-inbox",
            content: "first-note",
            createdAt: "2026-03-16T00:00:00Z",
          },
        ],
      }),
    );

    expect(markup).toContain("Back to list");
    expect(markup).toContain("first-note");
    expect(markup).toContain("2026-03-16T00:00:00Z");
    expect(markup).toContain("Read-only timeline");
  });

  it("renders archived list state with archived badges and readonly hint", () => {
    const markup = renderShellMarkup(
      buildShell({
        seriesCollection: "archived",
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
        selectedSeriesId: "series-archive",
        activeSelectedSeriesId: "series-inbox",
        archivedSelectedSeriesId: "series-archive",
      }),
    );

    expect(markup).toContain("Archived Series");
    expect(markup).toContain("Archived");
    expect(markup).toContain("Archived series stay read-only.");
    expect(markup).not.toContain("Create a new series");
    expect(markup).not.toContain("Append commit to");
  });

  it("renders the series empty state", () => {
    const markup = renderShellMarkup(
      buildShell({
        seriesList: [],
        selectedSeriesId: null,
      }),
    );

    expect(markup).toContain("No series yet");
    expect(markup).toContain("series.list");
  });

  it("renders the archived empty state", () => {
    const markup = renderShellMarkup(
      buildShell({
        seriesCollection: "archived",
        seriesList: [],
        selectedSeriesId: null,
        activeSelectedSeriesId: "series-inbox",
        archivedSelectedSeriesId: null,
      }),
    );

    expect(markup).toContain("No archived series");
    expect(markup).toContain("press `a` on a silent series");
  });

  it("renders the timeline error state", () => {
    const markup = renderShellMarkup(
      buildShell({
        view: "timeline",
        activeTimelineSeries: {
          id: "series-inbox",
          name: "Inbox",
          status: "active",
          lastUpdatedAt: "2026-03-16T00:00:00Z",
          latestExcerpt: "first-note",
          createdAt: "2026-03-15T00:00:00Z",
        },
        timelineLoadState: "error",
        navigationError: {
          code: "INVOKE_FAILED",
          message: "failed to load timeline",
        },
      }),
    );

    expect(markup).toContain("Retry");
    expect(markup).toContain("Return");
    expect(markup).toContain("INVOKE_FAILED");
  });

  it("renders the search, create, and commit command bars", () => {
    const searchMarkup = renderShellMarkup(
      buildShell({
        interactionMode: "search",
        searchQuery: "In",
        pendingAction: "search",
      }),
    );
    const createMarkup = renderShellMarkup(
      buildShell({
        interactionMode: "create_series",
        newSeriesNameDraft: "New Series",
        pendingAction: "create_series",
      }),
    );
    const commitMarkup = renderShellMarkup(
      buildShell({
        interactionMode: "draft_commit",
        commitDraft: "follow-up-note",
        pendingAction: "append_commit",
      }),
    );

    expect(searchMarkup).toContain("Search series");
    expect(searchMarkup).toContain("Searching...");
    expect(createMarkup).toContain("Create a new series");
    expect(createMarkup).toContain("Creating...");
    expect(commitMarkup).toContain("Append commit to Inbox");
    expect(commitMarkup).toContain("Saving...");
  });

  it("renders interaction feedback and archive pending state", () => {
    const markup = renderShellMarkup(
      buildShell({
        interactionFeedback: {
          code: "ARCHIVE_DISABLED",
          message: "only silent series can be archived with `a`",
        },
        pendingAction: "archive_series",
      }),
    );

    expect(markup).toContain("ARCHIVE_DISABLED");
    expect(markup).toContain("only silent series can be archived with `a`");
    expect(markup).toContain("Archiving the selected silent series...");
  });

  it("renders archived timeline badges", () => {
    const markup = renderShellMarkup(
      buildShell({
        view: "timeline",
        seriesCollection: "archived",
        activeTimelineSeries: {
          id: "series-archive",
          name: "Archive",
          status: "archived",
          lastUpdatedAt: "2026-03-17T00:00:00Z",
          latestExcerpt: "frozen note",
          createdAt: "2026-03-15T00:00:00Z",
          archivedAt: "2026-03-17T00:00:00Z",
        },
        selectedSeriesId: "series-archive",
        activeSelectedSeriesId: "series-inbox",
        archivedSelectedSeriesId: "series-archive",
        timelineLoadState: "ready",
        timelineItems: [
          {
            id: "commit-archive-1",
            seriesId: "series-archive",
            content: "frozen note",
            createdAt: "2026-03-16T00:00:00Z",
          },
        ],
      }),
    );

    expect(markup).toContain("Archived timeline is read-only");
    expect(markup).toContain('data-testid="timeline-archived-badge"');
  });

  it("renders silent rows with a visual status marker", () => {
    const markup = renderShellMarkup(buildShell());

    expect(markup).toContain('data-testid="series-status-series-project-a"');
    expect(markup).toContain("series-row is-silent");
  });
});
