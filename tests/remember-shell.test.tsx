import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ShellState } from "../src/application/types";
import { RememberShell } from "../src/ui/RememberShell";

function buildShell(overrides?: Partial<ShellState>): ShellState {
  return {
    appTitle: "Remember",
    subtitle: "Phase 4 Task 4 - Submit and Reorder",
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
      path: "series.create",
      envelope: {
        ok: true,
        data: {
          series: {
            id: "series-inbox",
            name: "Inbox",
            status: "active",
            lastUpdatedAt: "2026-03-16T00:00:00Z",
            latestExcerpt: "first-note",
            createdAt: "2026-03-15T00:00:00Z",
          },
        },
        meta: {
          path: "series.create",
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
    expect(markup).toContain("View timeline");
    expect(markup).toContain("Startup Self-Heal");
    expect(markup).toContain("No unresolved startup alerts.");
  });

  it("renders unresolved startup self-heal messages", () => {
    const shell = buildShell({
      commandProbe: {
        source: "mock",
        path: "series.create",
        envelope: {
          ok: false,
          error: {
            code: "DUAL_WRITE_FAILED",
            message: "simulated",
          },
          meta: {
            path: "series.create",
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
});
