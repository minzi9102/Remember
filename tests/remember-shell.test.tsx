import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ShellState } from "../src/application/types";
import { RememberShell } from "../src/ui/RememberShell";

function buildShell(overrides?: Partial<ShellState>): ShellState {
  return {
    appTitle: "Remember",
    subtitle: "SQLite-only shell",
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

const noop = () => undefined;

function renderShellMarkup(shell: ShellState, options?: { drawerOpen?: boolean }) {
  return renderToStaticMarkup(
    <RememberShell
      shell={shell}
      isDiagnosticsDrawerOpen={options?.drawerOpen === true}
      onToggleDiagnosticsDrawer={noop}
      onSelectCollection={noop}
      onSelectSeries={noop}
      onOpenTimeline={noop}
      onBackToList={noop}
      onRetryTimeline={noop}
      onSearchQueryChange={noop}
      onNewSeriesNameDraftChange={noop}
      onCommitDraftChange={noop}
      onCommitDraftCompositionStart={noop}
      onCommitDraftCompositionEnd={noop}
    />,
  );
}

describe("RememberShell sqlite-only views", () => {
  it("renders minimalist rail layout with floating controls", () => {
    const markup = renderShellMarkup(buildShell());

    expect(markup).toContain("Series");
    expect(markup).toContain("Inbox");
    expect(markup).toContain("Silent");
    expect(markup).toContain("data-testid=\"top-dock\"");
    expect(markup).toContain("data-testid=\"main-rail\"");
    expect(markup).toContain("data-testid=\"series-rail\"");
    expect(markup).toContain("data-testid=\"top-edge-controls\"");
    expect(markup).toContain("data-testid=\"top-edge-title\"");
    expect(markup).toContain("Remember");
    expect(markup).toContain("data-testid=\"view-toggle-container\"");
    expect(markup).toContain("data-testid=\"shortcut-hints-watermark\"");
    expect(markup).toContain("data-testid=\"workspace-glass-placeholder\"");
    expect(markup).toContain("class=\"workspace-stage cross-axis-stage\"");
    expect(markup).not.toContain("workspace-stage cross-axis-stage has-timeline-lane");
    expect(markup).toContain("data-testid=\"diagnostics-drawer-toggle\"");
    expect(markup).toContain("aria-controls=\"diagnostics-drawer-panel\"");
    expect(markup).toContain("aria-expanded=\"false\"");
    expect(markup).not.toContain("View timeline");
    expect(markup).not.toContain("Low-friction capture");
    expect(markup).not.toContain("UI: ready");
    expect(markup).not.toContain("Main Rail");
    expect(markup).not.toContain("Active Series");
    expect(markup).not.toContain("data-testid=\"selection-footer\"");
    expect(markup).not.toContain("data-testid=\"floating-corner-controls\"");
    expect(markup).toContain("Startup Self-Heal");
    expect(markup).toContain("No unresolved startup alerts.");
  });

  it("renders timeline preview lane in series list without back button", () => {
    const markup = renderShellMarkup(
      buildShell({
        view: "series_list",
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

    expect(markup).toContain("data-testid=\"timeline-lane\"");
    expect(markup).toContain("workspace-stage cross-axis-stage has-timeline-lane");
    expect(markup).toContain("Timeline preview. Double-click a card to focus.");
    expect(markup).toContain("first-note");
    expect(markup).not.toContain("data-testid=\"timeline-back-button\"");
  });

  it("renders unresolved startup self-heal messages", () => {
    const markup = renderShellMarkup(
      buildShell({
        commandProbe: {
          source: "mock",
          path: "series.list",
          envelope: {
            ok: false,
            error: {
              code: "INTERNAL_ERROR",
              message: "simulated",
            },
            meta: {
              path: "series.list",
              runtimeMode: "sqlite_only",
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
      }),
    );

    expect(markup).toContain("unresolved: 1");
    expect(markup).toContain("failed: 1");
    expect(markup).toContain("alert `a` remains unresolved");
  });

  it("opens the diagnostics drawer with disclosure semantics", () => {
    const markup = renderShellMarkup(buildShell(), { drawerOpen: true });

    expect(markup).toContain("diagnostics-drawer is-open");
    expect(markup).toContain("aria-expanded=\"true\"");
    expect(markup).toContain("aria-hidden=\"false\"");
  });

  it("renders timeline and archived states as read-only", () => {
    const timelineMarkup = renderShellMarkup(
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
    const archivedMarkup = renderShellMarkup(
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

    expect(timelineMarkup).toContain("Read-only timeline");
    expect(timelineMarkup).toContain("data-testid=\"timeline-lane\"");
    expect(timelineMarkup).toContain("first-note");
    expect(timelineMarkup).not.toContain("data-testid=\"workspace-glass-placeholder\"");
    expect(archivedMarkup).toContain("Archived series stay read-only.");
  });

  it("renders search/create/commit bars and archive feedback", () => {
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
    const feedbackMarkup = renderShellMarkup(
      buildShell({
        interactionFeedback: {
          code: "ARCHIVE_DISABLED",
          message: "only silent series can be archived with `a`",
        },
        pendingAction: "archive_series",
      }),
    );

    expect(searchMarkup).toContain("Searching...");
    expect(createMarkup).toContain("Creating...");
    expect(commitMarkup).toContain("Saving...");
    expect(feedbackMarkup).toContain("ARCHIVE_DISABLED");
    expect(feedbackMarkup).toContain("Archiving the selected silent series...");
  });
});
