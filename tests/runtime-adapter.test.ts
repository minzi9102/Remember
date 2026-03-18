import { describe, expect, it } from "vitest";

import {
  buildDefaultSilentScanRequest,
  readMockAppendCommit,
  readMockArchiveSeries,
  buildDefaultSeriesListRequest,
  buildDefaultTimelineRequest,
  parseMockRuntimeStatus,
  parseNativeRuntimeStatusFromTitle,
  readMockCommandProbe,
  readMockCreateSeries,
  readMockScanSilent,
  readMockSeriesList,
  readMockTimeline,
} from "../src/adapter/runtime-adapter";

let nextMockSession = 0;

function withMockSession(search: string): string {
  nextMockSession += 1;
  const separator = search.includes("?") ? "&" : "?";
  return `${search}${separator}mock_session=test-${nextMockSession}`;
}

describe("runtime-adapter mock parser", () => {
  it("keeps valid runtime mode without fallback", () => {
    const status = parseMockRuntimeStatus("?runtime_mode=dual_sync");

    expect(status.mode).toBe("dual_sync");
    expect(status.usedFallback).toBe(false);
    expect(status.warnings).toEqual([]);
    expect(status.source).toBe("mock");
  });

  it("falls back when runtime mode is invalid", () => {
    const status = parseMockRuntimeStatus("?runtime_mode=invalid_mode");

    expect(status.mode).toBe("sqlite_only");
    expect(status.usedFallback).toBe(true);
    expect(status.warnings.some((warning) => warning.includes("invalid runtime_mode"))).toBe(true);
  });

  it("collects warning list from query parameters", () => {
    const status = parseMockRuntimeStatus(
      "?runtime_mode=postgres_only&warning=manual-check-required&warnings=config-missing;fallback-on",
    );

    expect(status.mode).toBe("postgres_only");
    expect(status.usedFallback).toBe(false);
    expect(status.warnings).toContain("manual-check-required");
    expect(status.warnings).toContain("config-missing");
    expect(status.warnings).toContain("fallback-on");
  });
});

describe("runtime-adapter native title parser", () => {
  it("parses runtime mode marker from title", () => {
    const status = parseNativeRuntimeStatusFromTitle("Remember [postgres_only]");

    expect(status.mode).toBe("postgres_only");
    expect(status.usedFallback).toBe(false);
    expect(status.warnings).toEqual([]);
    expect(status.source).toBe("native");
  });

  it("marks fallback when title includes CONFIG_FALLBACK", () => {
    const status = parseNativeRuntimeStatusFromTitle("Remember [sqlite_only] [CONFIG_FALLBACK]");

    expect(status.mode).toBe("sqlite_only");
    expect(status.usedFallback).toBe(true);
    expect(status.warnings).toContain("native runtime reports CONFIG_FALLBACK");
  });

  it("adds warning when title includes HOTKEY_DISABLED marker", () => {
    const status = parseNativeRuntimeStatusFromTitle("Remember [dual_sync] [HOTKEY_DISABLED]");

    expect(status.mode).toBe("dual_sync");
    expect(status.usedFallback).toBe(false);
    expect(status.warnings).toContain(
      "native runtime reports HOTKEY_DISABLED (global hotkey disabled)",
    );
  });
});

describe("runtime-adapter command envelope probe", () => {
  it("returns success envelope in mock mode by default", () => {
    const probe = readMockCommandProbe(withMockSession("?runtime_mode=dual_sync"));

    expect(probe.path).toBe("series.create");
    expect(probe.source).toBe("mock");
    expect(probe.envelope.ok).toBe(true);
    expect(probe.envelope.meta.runtimeMode).toBe("dual_sync");
    expect(probe.envelope.meta.path).toBe("series.create");
    expect(probe.envelope.meta.startupSelfHeal).toMatchObject({
      scannedAlerts: 0,
      repairedAlerts: 0,
      unresolvedAlerts: 0,
      failedAlerts: 0,
      messages: [],
    });
    expect(probe.envelope.data).toMatchObject({
      series: {
        name: "Inbox",
        status: "active",
      },
    });
    expect((probe.envelope.data as { series: { id: string } }).series.id).toMatch(
      /^stub-series-inbox/,
    );
  });

  it("returns DTO fields for series.list", () => {
    const probe = readMockCommandProbe(
      withMockSession("?runtime_mode=sqlite_only&rpc_path=series.list"),
    );

    expect(probe.envelope.ok).toBe(true);
    expect(probe.envelope.data).toMatchObject({
      items: [
        {
          id: "series-inbox",
          name: "Inbox",
          status: "active",
        },
        {
          id: "series-project-a",
          name: "Project-A",
          status: "silent",
        },
      ],
      nextCursor: null,
      limitEcho: 50,
    });
  });

  it("returns DTO fields for commit.append", () => {
    const probe = readMockCommandProbe(
      withMockSession("?runtime_mode=sqlite_only&rpc_path=commit.append"),
    );

    expect(probe.envelope.ok).toBe(true);
    expect(probe.envelope.data).toMatchObject({
      commit: {
        seriesId: "series-inbox",
        content: "first-note",
      },
      series: {
        id: "series-inbox",
        name: "Inbox",
        status: "active",
      },
    });
    expect((probe.envelope.data as { commit: { id: string } }).commit.id).toMatch(
      /^stub-commit-/,
    );
  });

  it("returns DTO fields for timeline.list", () => {
    const probe = readMockCommandProbe(
      withMockSession("?runtime_mode=sqlite_only&rpc_path=timeline.list"),
    );

    expect(probe.envelope.ok).toBe(true);
    expect(probe.envelope.data).toMatchObject({
      seriesId: "series-inbox",
      items: [
        {
          id: "stub-commit-001",
          seriesId: "series-inbox",
          content: "first-note",
          createdAt: "2026-03-16T00:00:00Z",
        },
      ],
      nextCursor: null,
    });
  });

  it("returns DTO fields for series.scan_silent", () => {
    const probe = readMockCommandProbe(
      withMockSession("?runtime_mode=sqlite_only&rpc_path=series.scan_silent"),
    );

    expect(probe.envelope.ok).toBe(true);
    expect(probe.envelope.data).toMatchObject({
      affectedSeriesIds: [],
      thresholdDays: 7,
    });
  });

  it("returns validation error when rpc_fail is enabled", () => {
    const probe = readMockCommandProbe(withMockSession("?runtime_mode=sqlite_only&rpc_fail=1"));

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("returns pg timeout error when rpc_error is pg_timeout", () => {
    const probe = readMockCommandProbe(
      withMockSession("?runtime_mode=dual_sync&rpc_error=pg_timeout"),
    );

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("PG_TIMEOUT");
  });

  it("returns dual write failed error when rpc_error is dual_write_failed", () => {
    const probe = readMockCommandProbe(
      withMockSession("?runtime_mode=dual_sync&rpc_error=dual_write_failed"),
    );

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("DUAL_WRITE_FAILED");
  });

  it("returns unknown command error for unsupported path", () => {
    const probe = readMockCommandProbe(
      withMockSession("?runtime_mode=sqlite_only&rpc_path=series.unknown"),
    );

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("UNKNOWN_COMMAND");
  });

  it("parses startup self-heal summary from query parameters", () => {
    const probe = readMockCommandProbe(
      withMockSession(
        "?runtime_mode=dual_sync&startup_self_heal_scanned=4&startup_self_heal_repaired=3&startup_self_heal_unresolved=1&startup_self_heal_failed=1&startup_self_heal_message=alert-a&startup_self_heal_messages=alert-b;alert-c",
      ),
    );

    expect(probe.envelope.meta.startupSelfHeal).toMatchObject({
      scannedAlerts: 4,
      repairedAlerts: 3,
      unresolvedAlerts: 1,
      failedAlerts: 1,
      messages: ["alert-a", "alert-b", "alert-c"],
    });
  });
});

describe("runtime-adapter typed helpers", () => {
  it("returns create series data in mock mode", () => {
    const envelope = readMockCreateSeries(withMockSession("?runtime_mode=sqlite_only"), "Inbox");

    expect(envelope.ok).toBe(true);
    expect(envelope.data?.series).toMatchObject({ name: "Inbox", status: "active" });
    expect(envelope.data?.series.id).toMatch(/^stub-series-inbox/);
  });

  it("returns validation error for create series when mock fail flag is enabled", () => {
    const envelope = readMockCreateSeries(
      withMockSession("?runtime_mode=sqlite_only&rpc_fail=1"),
      "Inbox",
    );

    expect(envelope.ok).toBe(false);
    expect(envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("returns series list data in mock mode", () => {
    const envelope = readMockSeriesList(
      withMockSession("?runtime_mode=sqlite_only"),
      buildDefaultSeriesListRequest(),
    );

    expect(envelope.ok).toBe(true);
    expect(envelope.data?.items).toHaveLength(2);
    expect(envelope.data?.items[0]?.id).toBe("series-inbox");
  });

  it("returns validation error for series list when mock fail flag is enabled", () => {
    const envelope = readMockSeriesList(
      withMockSession("?runtime_mode=sqlite_only&rpc_fail=1"),
      buildDefaultSeriesListRequest(),
    );

    expect(envelope.ok).toBe(false);
    expect(envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("returns commit append data in mock mode", () => {
    const envelope = readMockAppendCommit(
      withMockSession("?runtime_mode=sqlite_only"),
      "series-inbox",
      "follow-up-note",
      "2026-03-16T00:00:00Z",
    );

    expect(envelope.ok).toBe(true);
    expect(envelope.data).toMatchObject({
      commit: {
        seriesId: "series-inbox",
        content: "follow-up-note",
        createdAt: "2026-03-16T00:00:00Z",
      },
      series: {
        id: "series-inbox",
        status: "active",
        latestExcerpt: "follow-up-note",
      },
    });
    expect(envelope.data?.commit.id).toMatch(/^stub-commit-/);
  });

  it("returns archive data in mock mode", () => {
    const envelope = readMockArchiveSeries(
      withMockSession("?runtime_mode=sqlite_only"),
      "series-project-a",
    );

    expect(envelope.ok).toBe(true);
    expect(envelope.data).toMatchObject({
      seriesId: "series-project-a",
      archivedAt: "2026-03-16T12:00:00Z",
    });
  });

  it("keeps archived series out of the active list while exposing them through includeArchived", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");

    const archived = readMockArchiveSeries(search, "series-project-a");
    const activeList = readMockSeriesList(search, buildDefaultSeriesListRequest());
    const allSeries = readMockSeriesList(search, {
      ...buildDefaultSeriesListRequest(),
      includeArchived: true,
    });

    expect(archived.ok).toBe(true);
    expect(activeList.data?.items.some((item) => item.id === "series-project-a")).toBe(false);
    expect(allSeries.data?.items.find((item) => item.id === "series-project-a")).toMatchObject({
      status: "archived",
      archivedAt: "2026-03-16T12:00:00Z",
    });
  });

  it("returns archived series in includeArchived searches", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");

    readMockArchiveSeries(search, "series-project-a");
    const archivedSearch = readMockSeriesList(search, {
      ...buildDefaultSeriesListRequest(),
      includeArchived: true,
      query: "Project",
    });

    expect(archivedSearch.ok).toBe(true);
    expect(archivedSearch.data?.items.find((item) => item.id === "series-project-a")).toMatchObject({
      status: "archived",
    });
  });

  it("returns forced error for archive helper", () => {
    const envelope = readMockArchiveSeries(
      withMockSession("?runtime_mode=dual_sync&rpc_error=dual_write_failed"),
      "series-project-a",
    );

    expect(envelope.ok).toBe(false);
    expect(envelope.error?.code).toBe("DUAL_WRITE_FAILED");
  });

  it("returns silent scan data in mock mode", () => {
    const envelope = readMockScanSilent(
      withMockSession("?runtime_mode=sqlite_only"),
      buildDefaultSilentScanRequest("2026-03-24T00:00:00Z"),
    );

    expect(envelope.ok).toBe(true);
    expect(envelope.data).toMatchObject({
      affectedSeriesIds: ["series-inbox"],
      thresholdDays: 7,
    });
  });

  it("returns forced error for silent scan helper", () => {
    const envelope = readMockScanSilent(
      withMockSession("?runtime_mode=dual_sync&rpc_error=dual_write_failed"),
      buildDefaultSilentScanRequest("2026-03-24T00:00:00Z"),
    );

    expect(envelope.ok).toBe(false);
    expect(envelope.error?.code).toBe("DUAL_WRITE_FAILED");
  });

  it("limits forced errors to the configured rpc path when rpc_error_path is set", () => {
    const search = withMockSession(
      "?runtime_mode=sqlite_only&rpc_error=dual_write_failed&rpc_error_path=series.scan_silent",
    );

    const scanned = readMockScanSilent(search, buildDefaultSilentScanRequest("2026-03-24T00:00:00Z"));
    const listed = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(scanned.ok).toBe(false);
    expect(scanned.error?.code).toBe("DUAL_WRITE_FAILED");
    expect(listed.ok).toBe(true);
  });

  it("returns forced error for timeline helper", () => {
    const envelope = readMockTimeline(
      withMockSession("?runtime_mode=dual_sync&rpc_error=dual_write_failed"),
      "series-inbox",
      buildDefaultTimelineRequest(),
    );

    expect(envelope.ok).toBe(false);
    expect(envelope.error?.code).toBe("DUAL_WRITE_FAILED");
  });

  it("returns timeline items for the requested series", () => {
    const envelope = readMockTimeline(
      withMockSession("?runtime_mode=sqlite_only"),
      "series-project-a",
      buildDefaultTimelineRequest(),
    );

    expect(envelope.ok).toBe(true);
    expect(envelope.data?.seriesId).toBe("series-project-a");
    expect(envelope.data?.items).toMatchObject([
      {
        id: "stub-commit-002",
        content: "follow-up-note",
      },
      {
        id: "stub-commit-003",
        content: "first-project-note",
      },
    ]);
  });

  it("keeps archived timelines readable after archiving in the same mock session", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");

    const archived = readMockArchiveSeries(search, "series-project-a");
    const timeline = readMockTimeline(search, "series-project-a", buildDefaultTimelineRequest());

    expect(archived.ok).toBe(true);
    expect(timeline.ok).toBe(true);
    expect(timeline.data?.items).toHaveLength(2);
    expect(timeline.data?.items[0]).toMatchObject({
      id: "stub-commit-002",
      content: "follow-up-note",
    });
  });

  it("keeps created series and latest commit excerpt in the same mock session", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");
    const created = readMockCreateSeries(search, "Roadmap");
    const createdSeriesId = created.data?.series.id;

    expect(created.ok).toBe(true);
    expect(createdSeriesId).toBeTruthy();

    const appended = readMockAppendCommit(
      search,
      createdSeriesId ?? "",
      "roadmap follow-up note",
      "2026-03-18T02:00:00+08:00",
    );
    const listed = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(appended.ok).toBe(true);
    expect(appended.data?.series.lastUpdatedAt).toBe("2026-03-17T18:00:00Z");
    expect(listed.data?.items[0]).toMatchObject({
      id: createdSeriesId,
      latestExcerpt: "roadmap follow-up note",
      lastUpdatedAt: "2026-03-17T18:00:00Z",
    });
  });

  it("reorders series by the most recent append in the same mock session", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");

    const firstAppend = readMockAppendCommit(
      search,
      "series-project-a",
      "project-a now leads",
      "2026-03-18T01:00:00Z",
    );
    const afterFirst = readMockSeriesList(search, buildDefaultSeriesListRequest());
    const secondAppend = readMockAppendCommit(
      search,
      "series-inbox",
      "inbox reclaims top",
      "2026-03-18T02:00:00Z",
    );
    const afterSecond = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(firstAppend.ok).toBe(true);
    expect(secondAppend.ok).toBe(true);
    expect(afterFirst.data?.items[0]?.id).toBe("series-project-a");
    expect(afterSecond.data?.items[0]).toMatchObject({
      id: "series-inbox",
      latestExcerpt: "inbox reclaims top",
      lastUpdatedAt: "2026-03-18T02:00:00Z",
    });
  });

  it("shows newly appended commits in timeline reads for the same mock session", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");

    const appended = readMockAppendCommit(
      search,
      "series-project-a",
      "timeline verification note",
      "2026-03-18T03:00:00Z",
    );
    const timeline = readMockTimeline(search, "series-project-a", buildDefaultTimelineRequest());

    expect(appended.ok).toBe(true);
    expect(timeline.ok).toBe(true);
    expect(timeline.data?.items[0]).toMatchObject({
      content: "timeline verification note",
      createdAt: "2026-03-18T03:00:00Z",
    });
  });

  it("marks stale active series as silent after an explicit scan in the same mock session", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");

    const scanned = readMockScanSilent(
      search,
      buildDefaultSilentScanRequest("2026-03-24T00:00:00Z"),
    );
    const listed = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(scanned.ok).toBe(true);
    expect(scanned.data?.affectedSeriesIds).toEqual(["series-inbox"]);
    expect(listed.data?.items.find((item) => item.id === "series-inbox")).toMatchObject({
      status: "silent",
    });
  });

  it("reactivates a silent series after a new commit in the same mock session", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");

    const scanned = readMockScanSilent(
      search,
      buildDefaultSilentScanRequest("2026-03-24T00:00:00Z"),
    );
    const appended = readMockAppendCommit(
      search,
      "series-project-a",
      "wake up again",
      "2026-03-24T01:00:00Z",
    );
    const listed = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(scanned.ok).toBe(true);
    expect(appended.ok).toBe(true);
    expect(appended.data?.series).toMatchObject({
      id: "series-project-a",
      status: "active",
      latestExcerpt: "wake up again",
    });
    expect(listed.data?.items[0]).toMatchObject({
      id: "series-project-a",
      status: "active",
      latestExcerpt: "wake up again",
    });
  });

  it("does not mutate the mock store when append validation fails", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");
    const before = readMockSeriesList(search, buildDefaultSeriesListRequest());
    const failed = readMockAppendCommit(search, "series-inbox", "", "2026-03-18T04:00:00Z");
    const after = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(failed.ok).toBe(false);
    expect(failed.error?.code).toBe("VALIDATION_ERROR");
    expect(after.data?.items).toEqual(before.data?.items);
  });

  it("does not mutate the mock store when append is force-failed", () => {
    const search = withMockSession("?runtime_mode=dual_sync");
    const failedSearch = `${search}&rpc_error=dual_write_failed`;
    const before = readMockSeriesList(search, buildDefaultSeriesListRequest());
    const failed = readMockAppendCommit(
      failedSearch,
      "series-project-a",
      "should not persist",
      "2026-03-18T05:00:00Z",
    );
    const after = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(failed.ok).toBe(false);
    expect(failed.error?.code).toBe("DUAL_WRITE_FAILED");
    expect(after.data?.items).toEqual(before.data?.items);
  });
});
