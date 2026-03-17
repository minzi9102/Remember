import { describe, expect, it } from "vitest";

import {
  parseMockRuntimeStatus,
  parseNativeRuntimeStatusFromTitle,
  readMockCommandProbe,
} from "../src/adapter/runtime-adapter";

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
});

describe("runtime-adapter command envelope probe", () => {
  it("returns success envelope in mock mode by default", () => {
    const probe = readMockCommandProbe("?runtime_mode=dual_sync");

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
        id: "stub-series-inbox",
        name: "Inbox",
        status: "active",
      },
    });
  });

  it("returns DTO fields for series.list", () => {
    const probe = readMockCommandProbe("?runtime_mode=sqlite_only&rpc_path=series.list");

    expect(probe.envelope.ok).toBe(true);
    expect(probe.envelope.data).toMatchObject({
      items: [
        {
          id: "series-inbox",
          name: "Inbox",
          status: "active",
          lastUpdatedAt: "2026-03-16T00:00:00Z",
          latestExcerpt: "first-note",
          createdAt: "2026-03-15T00:00:00Z",
        },
      ],
      nextCursor: null,
      limitEcho: 50,
    });
  });

  it("returns DTO fields for commit.append", () => {
    const probe = readMockCommandProbe("?runtime_mode=sqlite_only&rpc_path=commit.append");

    expect(probe.envelope.ok).toBe(true);
    expect(probe.envelope.data).toMatchObject({
      commit: {
        id: "stub-commit-001",
        seriesId: "series-inbox",
        content: "first-note",
      },
      series: {
        id: "series-inbox",
        name: "Stub Series",
        status: "active",
      },
    });
  });

  it("returns DTO fields for timeline.list", () => {
    const probe = readMockCommandProbe("?runtime_mode=sqlite_only&rpc_path=timeline.list");

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

  it("returns validation error when rpc_fail is enabled", () => {
    const probe = readMockCommandProbe("?runtime_mode=sqlite_only&rpc_fail=1");

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("VALIDATION_ERROR");
  });

  it("returns pg timeout error when rpc_error is pg_timeout", () => {
    const probe = readMockCommandProbe("?runtime_mode=dual_sync&rpc_error=pg_timeout");

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("PG_TIMEOUT");
  });

  it("returns dual write failed error when rpc_error is dual_write_failed", () => {
    const probe = readMockCommandProbe("?runtime_mode=dual_sync&rpc_error=dual_write_failed");

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("DUAL_WRITE_FAILED");
  });

  it("returns unknown command error for unsupported path", () => {
    const probe = readMockCommandProbe("?runtime_mode=sqlite_only&rpc_path=series.unknown");

    expect(probe.envelope.ok).toBe(false);
    expect(probe.envelope.error?.code).toBe("UNKNOWN_COMMAND");
  });

  it("parses startup self-heal summary from query parameters", () => {
    const probe = readMockCommandProbe(
      "?runtime_mode=dual_sync&startup_self_heal_scanned=4&startup_self_heal_repaired=3&startup_self_heal_unresolved=1&startup_self_heal_failed=1&startup_self_heal_message=alert-a&startup_self_heal_messages=alert-b;alert-c",
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
