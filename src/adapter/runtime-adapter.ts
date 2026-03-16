import type { LayerState, RuntimeMode, RuntimeStatus } from "../application/types";

export interface AdapterSnapshot {
  adapter: LayerState;
  repository: LayerState;
  runtimeStatus: RuntimeStatus;
}

const DEFAULT_MODE: RuntimeMode = "sqlite_only";
const RUNTIME_MODE_PATTERN = /\[(sqlite_only|postgres_only|dual_sync)\]/;
const FALLBACK_PATTERN = /\[CONFIG_FALLBACK\]/;

export function parseMockRuntimeStatus(search: string): RuntimeStatus {
  const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search);
  const runtimeMode = params.get("runtime_mode");
  const fallbackFlag = params.get("fallback");
  const warnings = collectWarningsFromParams(params);
  const normalized = normalizeRuntimeMode(runtimeMode);

  if (normalized.usedFallback && normalized.warning !== null) {
    warnings.push(normalized.warning);
  }

  return {
    mode: normalized.mode,
    usedFallback: fallbackFlag === "1" || fallbackFlag === "true" || normalized.usedFallback,
    warnings: uniqueWarnings(warnings),
    source: "mock",
  };
}

export function parseNativeRuntimeStatusFromTitle(title: string): RuntimeStatus {
  const modeMatch = title.match(RUNTIME_MODE_PATTERN);
  const fallbackMarkExists = FALLBACK_PATTERN.test(title);
  const warnings: string[] = [];

  let mode: RuntimeMode = DEFAULT_MODE;
  let usedFallback = fallbackMarkExists;

  if (modeMatch?.[1]) {
    const normalized = normalizeRuntimeMode(modeMatch[1]);
    mode = normalized.mode;
    if (normalized.usedFallback && normalized.warning !== null) {
      warnings.push(normalized.warning);
      usedFallback = true;
    }
  } else {
    warnings.push("runtime mode marker missing in native title, fallback to sqlite_only");
    usedFallback = true;
  }

  if (fallbackMarkExists) {
    warnings.push("native runtime reports CONFIG_FALLBACK");
  }

  return {
    mode,
    usedFallback,
    warnings: uniqueWarnings(warnings),
    source: "native",
  };
}

export async function readRuntimeStatus(): Promise<RuntimeStatus> {
  if (!isTauriRuntime()) {
    return parseMockRuntimeStatus(window.location.search);
  }

  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const title = await getCurrentWindow().title();
    return parseNativeRuntimeStatusFromTitle(title);
  } catch (error) {
    return {
      mode: DEFAULT_MODE,
      usedFallback: true,
      warnings: [`failed to read native window title, fallback to sqlite_only: ${String(error)}`],
      source: "native",
    };
  }
}

export async function readAdapterSnapshot(): Promise<AdapterSnapshot> {
  return {
    adapter: "ready",
    repository: "stubbed",
    runtimeStatus: await readRuntimeStatus(),
  };
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function collectWarningsFromParams(params: URLSearchParams): string[] {
  const warnings: string[] = [];

  for (const warning of params.getAll("warning")) {
    const trimmed = warning.trim();
    if (trimmed.length > 0) {
      warnings.push(trimmed);
    }
  }

  const warningsCsv = params.get("warnings");
  if (warningsCsv) {
    const splitWarnings = warningsCsv
      .split(/[;,]/)
      .map((item) => item.trim())
      .filter((item) => item.length > 0);
    warnings.push(...splitWarnings);
  }

  return warnings;
}

function normalizeRuntimeMode(
  rawMode: string | null,
): { mode: RuntimeMode; usedFallback: boolean; warning: string | null } {
  if (rawMode === "sqlite_only" || rawMode === "postgres_only" || rawMode === "dual_sync") {
    return { mode: rawMode, usedFallback: false, warning: null };
  }

  if (rawMode === null || rawMode.trim().length === 0) {
    return {
      mode: DEFAULT_MODE,
      usedFallback: true,
      warning: "missing runtime_mode, fallback to sqlite_only",
    };
  }

  return {
    mode: DEFAULT_MODE,
    usedFallback: true,
    warning: `invalid runtime_mode \`${rawMode}\`, fallback to sqlite_only`,
  };
}

function uniqueWarnings(warnings: string[]): string[] {
  return [...new Set(warnings)];
}
