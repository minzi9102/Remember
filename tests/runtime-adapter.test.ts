import { describe, expect, it } from "vitest";

import {
  parseMockRuntimeStatus,
  parseNativeRuntimeStatusFromTitle,
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
