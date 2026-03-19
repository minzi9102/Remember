import { describe, expect, it } from "vitest";

import {
  buildDefaultSeriesListRequest,
  buildDefaultSilentScanRequest,
  buildDefaultTimelineRequest,
  parseMockRuntimeStatus,
  parseNativeRuntimeStatusFromTitle,
  readMockAppendCommit,
  readMockArchiveSeries,
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

describe("runtime-adapter runtime status", () => {
  it("keeps sqlite_only mode without fallback", () => {
    const status = parseMockRuntimeStatus("?runtime_mode=sqlite_only");

    expect(status.mode).toBe("sqlite_only");
    expect(status.usedFallback).toBe(false);
    expect(status.warnings).toEqual([]);
    expect(status.source).toBe("mock");
  });

  it("warns when legacy runtime modes are present", () => {
    const status = parseMockRuntimeStatus("?runtime_mode=dual_sync");

    expect(status.mode).toBe("sqlite_only");
    expect(status.usedFallback).toBe(false);
    expect(status.warnings).toContain(
      "legacy runtime_mode `dual_sync` ignored; sqlite_only is always active",
    );
  });

  it("keeps warning collection from query parameters", () => {
    const status = parseMockRuntimeStatus(
      "?runtime_mode=postgres_only&warning=manual-check-required&warnings=config-missing;fallback-on",
    );

    expect(status.mode).toBe("sqlite_only");
    expect(status.warnings).toContain(
      "legacy runtime_mode `postgres_only` ignored; sqlite_only is always active",
    );
    expect(status.warnings).toContain("manual-check-required");
    expect(status.warnings).toContain("config-missing");
    expect(status.warnings).toContain("fallback-on");
  });

  it("parses native sqlite titles and fallback markers", () => {
    const okStatus = parseNativeRuntimeStatusFromTitle("Remember [sqlite_only]");
    const fallbackStatus = parseNativeRuntimeStatusFromTitle(
      "Remember [sqlite_only] [CONFIG_FALLBACK] [HOTKEY_DISABLED]",
    );

    expect(okStatus.mode).toBe("sqlite_only");
    expect(okStatus.usedFallback).toBe(false);
    expect(fallbackStatus.usedFallback).toBe(true);
    expect(fallbackStatus.warnings).toContain("native runtime reports CONFIG_FALLBACK");
    expect(fallbackStatus.warnings).toContain(
      "native runtime reports HOTKEY_DISABLED (global hotkey disabled)",
    );
  });

  it("warns when native titles contain legacy runtime markers", () => {
    const status = parseNativeRuntimeStatusFromTitle("Remember [postgres_only]");

    expect(status.mode).toBe("sqlite_only");
    expect(status.usedFallback).toBe(false);
    expect(status.warnings).toContain(
      "legacy runtime_mode `postgres_only` ignored; sqlite_only is always active",
    );
  });
});

describe("runtime-adapter mock probe and helpers", () => {
  it("returns the default command probe in sqlite mode", () => {
    const probe = readMockCommandProbe(withMockSession("?runtime_mode=sqlite_only"));

    expect(probe.path).toBe("series.list");
    expect(probe.source).toBe("mock");
    expect(probe.envelope.ok).toBe(true);
    expect(probe.envelope.meta.runtimeMode).toBe("sqlite_only");
    expect(probe.envelope.meta.startupSelfHeal).toMatchObject({
      scannedAlerts: 0,
      repairedAlerts: 0,
      unresolvedAlerts: 0,
      failedAlerts: 0,
      messages: [],
    });
  });

  it("returns validation errors when forced by rpc_fail", () => {
    const probe = readMockCommandProbe(withMockSession("?runtime_mode=sqlite_only&rpc_fail=1"));

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("keeps startup self-heal metadata available", () => {
    const probe = readMockCommandProbe(
      withMockSession(
        "?runtime_mode=sqlite_only&startup_self_heal_scanned=2&startup_self_heal_repaired=1&startup_self_heal_unresolved=1&startup_self_heal_failed=0&startup_self_heal_message=alert-a",
      ),
    );

    expect(probe.envelope.meta.startupSelfHeal).toMatchObject({
      scannedAlerts: 2,
      repairedAlerts: 1,
      unresolvedAlerts: 1,
      failedAlerts: 0,
      messages: ["alert-a"],
    });
  });

  it("supports create/list/append/timeline/archive/scan in one sqlite mock session", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");
    const created = readMockCreateSeries(search, "Roadmap");
    const createdSeriesId = created.data?.series.id ?? "";
    const appended = readMockAppendCommit(
      search,
      createdSeriesId,
      "roadmap follow-up note",
      "2026-03-18T02:00:00+08:00",
    );
    const listed = readMockSeriesList(search, buildDefaultSeriesListRequest());
    const timeline = readMockTimeline(search, createdSeriesId, buildDefaultTimelineRequest());
    const archived = readMockArchiveSeries(search, createdSeriesId);
    const archivedList = readMockSeriesList(search, {
      ...buildDefaultSeriesListRequest(),
      includeArchived: true,
    });
    const scanned = readMockScanSilent(
      search,
      buildDefaultSilentScanRequest("2026-03-24T00:00:00Z"),
    );

    expect(created.ok).toBe(true);
    expect(appended.ok).toBe(true);
    expect(listed.data?.items[0]).toMatchObject({
      id: createdSeriesId,
      latestExcerpt: "roadmap follow-up note",
      lastUpdatedAt: "2026-03-17T18:00:00Z",
    });
    expect(timeline.ok).toBe(true);
    expect(timeline.data?.items[0]).toMatchObject({
      content: "roadmap follow-up note",
      createdAt: "2026-03-17T18:00:00Z",
    });
    expect(archived.ok).toBe(true);
    expect(archivedList.data?.items.find((item) => item.id === createdSeriesId)).toMatchObject({
      status: "archived",
    });
    expect(scanned.ok).toBe(true);
  });

  it("scopes validation_error to the configured rpc path", () => {
    const search = withMockSession(
      "?runtime_mode=sqlite_only&rpc_error=validation_error&rpc_error_path=series.scan_silent",
    );

    const scanned = readMockScanSilent(search, buildDefaultSilentScanRequest("2026-03-24T00:00:00Z"));
    const listed = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(scanned.ok).toBe(false);
    expect(scanned.error?.code).toBe("VALIDATION_ERROR");
    expect(listed.ok).toBe(true);
  });

  it("does not mutate the store when append validation fails", () => {
    const search = withMockSession("?runtime_mode=sqlite_only");
    const before = readMockSeriesList(search, buildDefaultSeriesListRequest());
    const failed = readMockAppendCommit(search, "series-inbox", "", "2026-03-18T04:00:00Z");
    const after = readMockSeriesList(search, buildDefaultSeriesListRequest());

    expect(failed.ok).toBe(false);
    expect(failed.error?.code).toBe("VALIDATION_ERROR");
    expect(after.data?.items).toEqual(before.data?.items);
  });
});
