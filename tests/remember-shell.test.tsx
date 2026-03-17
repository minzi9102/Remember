import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ShellState } from "../src/application/types";
import { RememberShell } from "../src/ui/RememberShell";

function buildShell(overrides?: Partial<ShellState>): ShellState {
  return {
    appTitle: "Remember",
    subtitle: "Diagnostics",
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
    seriesPreview: [],
    timelinePreview: [],
    ...overrides,
  };
}

describe("RememberShell startup self-heal diagnostics", () => {
  it("renders clean startup self-heal summary", () => {
    const markup = renderToStaticMarkup(<RememberShell shell={buildShell()} />);

    expect(markup).toContain("Startup Self-Heal");
    expect(markup).toContain("scanned: 0");
    expect(markup).toContain("repaired: 0");
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

    const markup = renderToStaticMarkup(<RememberShell shell={shell} />);

    expect(markup).toContain("unresolved: 1");
    expect(markup).toContain("failed: 1");
    expect(markup).toContain("Unresolved startup alerts");
    expect(markup).toContain("alert `a` remains unresolved");
  });
});
