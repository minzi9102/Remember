import type {
  CommandProbe,
  LayerState,
  RpcEnvelope,
  RuntimeMode,
  RuntimeStatus,
} from "../application/types";

export interface AdapterSnapshot {
  adapter: LayerState;
  repository: LayerState;
  runtimeStatus: RuntimeStatus;
  commandProbe: CommandProbe;
}

const DEFAULT_MODE: RuntimeMode = "sqlite_only";
const RUNTIME_MODE_PATTERN = /\[(sqlite_only|postgres_only|dual_sync)\]/;
const FALLBACK_PATTERN = /\[CONFIG_FALLBACK\]/;
const DEFAULT_PROBE_PATH = "series.create";
const VALIDATION_ERROR = "VALIDATION_ERROR";
const UNKNOWN_COMMAND = "UNKNOWN_COMMAND";
const INVOKE_FAILED = "INVOKE_FAILED";

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
  const runtimeStatus = await readRuntimeStatus();

  return {
    adapter: "ready",
    repository: "stubbed",
    runtimeStatus,
    commandProbe: await readCommandProbe(runtimeStatus),
  };
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function readMockCommandProbe(search: string): CommandProbe {
  const runtimeStatus = parseMockRuntimeStatus(search);
  const request = buildProbeRequest(search);

  return {
    source: "mock",
    path: request.path,
    envelope: mockInvoke(request.path, request.payload, runtimeStatus),
  };
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

async function readCommandProbe(runtimeStatus: RuntimeStatus): Promise<CommandProbe> {
  const search = typeof window === "undefined" ? "" : window.location.search;
  const request = buildProbeRequest(search);

  if (!isTauriRuntime()) {
    return {
      source: "mock",
      path: request.path,
      envelope: mockInvoke(request.path, request.payload, runtimeStatus),
    };
  }

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const envelope = await invoke<RpcEnvelope>("rpc_invoke", {
      path: request.path,
      payload: request.payload,
    });

    return {
      source: "native",
      path: request.path,
      envelope,
    };
  } catch (error) {
    return {
      source: "native",
      path: request.path,
      envelope: buildErrorEnvelope(
        request.path,
        runtimeStatus,
        INVOKE_FAILED,
        `failed to invoke native rpc shell: ${String(error)}`,
      ),
    };
  }
}

function buildProbeRequest(search: string): { path: string; payload: Record<string, unknown> } {
  const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search);
  const path = normalizeProbePath(params.get("rpc_path"));
  const forceFail = isTruthy(params.get("rpc_fail"));

  return {
    path,
    payload: forceFail ? buildFailPayload(path) : buildSuccessPayload(path),
  };
}

function normalizeProbePath(rawPath: string | null): string {
  if (rawPath === null) {
    return DEFAULT_PROBE_PATH;
  }

  const trimmed = rawPath.trim();
  return trimmed.length > 0 ? trimmed : DEFAULT_PROBE_PATH;
}

function isTruthy(raw: string | null): boolean {
  if (raw === null) {
    return false;
  }

  const normalized = raw.trim().toLowerCase();
  return normalized === "1" || normalized === "true" || normalized === "yes";
}

function buildSuccessPayload(path: string): Record<string, unknown> {
  switch (path) {
    case "series.create":
      return { name: "Inbox" };
    case "series.list":
      return { query: "", includeArchived: false, limit: 50 };
    case "commit.append":
      return { seriesId: "series-inbox", content: "first-note" };
    case "timeline.list":
      return { seriesId: "series-inbox", limit: 20 };
    case "series.archive":
      return { seriesId: "series-inbox" };
    case "series.scan_silent":
      return { thresholdDays: 7 };
    default:
      return {};
  }
}

function buildFailPayload(path: string): Record<string, unknown> {
  switch (path) {
    case "series.create":
      return { name: "" };
    case "series.list":
      return { limit: 0 };
    case "commit.append":
      return { seriesId: "", content: "" };
    case "timeline.list":
      return { seriesId: "" };
    case "series.archive":
      return { seriesId: "" };
    case "series.scan_silent":
      return { thresholdDays: 0 };
    default:
      return {};
  }
}

function mockInvoke(
  path: string,
  payload: Record<string, unknown>,
  runtimeStatus: RuntimeStatus,
): RpcEnvelope {
  const meta = {
    path,
    runtimeMode: runtimeStatus.mode,
    usedFallback: runtimeStatus.usedFallback,
    respondedAtUnixMs: Date.now(),
  };

  try {
    const data = mockDispatch(path, payload);
    return {
      ok: true,
      data,
      meta,
    };
  } catch (error) {
    const rpcError =
      typeof error === "object" &&
      error !== null &&
      "code" in error &&
      "message" in error &&
      typeof (error as { code: unknown }).code === "string" &&
      typeof (error as { message: unknown }).message === "string"
        ? (error as { code: string; message: string })
        : { code: INVOKE_FAILED, message: `mock rpc dispatch failed: ${String(error)}` };

    return {
      ok: false,
      error: rpcError,
      meta,
    };
  }
}

function mockDispatch(path: string, payload: Record<string, unknown>): Record<string, unknown> {
  switch (path) {
    case "series.create": {
      const name = requireNonEmptyString(payload, "name");
      return {
        series: {
          id: "stub-series-inbox",
          name,
          status: "active",
          lastUpdatedAt: "2026-03-16T00:00:00Z",
          latestExcerpt: "stubbed-command-shell",
          createdAt: "2026-03-16T00:00:00Z",
        },
      };
    }
    case "series.list": {
      const limit = readOptionalPositiveInteger(payload, "limit") ?? 50;
      return {
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
        limitEcho: limit,
      };
    }
    case "commit.append": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      const content = requireNonEmptyString(payload, "content");
      return {
        commit: {
          id: "stub-commit-001",
          seriesId,
          content,
          createdAt: "2026-03-16T00:00:00Z",
        },
        series: {
          id: seriesId,
          name: "Stub Series",
          status: "active",
          lastUpdatedAt: "2026-03-16T00:00:00Z",
          latestExcerpt: content,
          createdAt: "2026-03-15T00:00:00Z",
        },
      };
    }
    case "timeline.list": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      return {
        seriesId,
        items: [
          {
            id: "stub-commit-001",
            seriesId,
            content: "first-note",
            createdAt: "2026-03-16T00:00:00Z",
          },
        ],
        nextCursor: null,
      };
    }
    case "series.archive": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      return {
        seriesId,
        archivedAt: "2026-03-16T00:00:00Z",
      };
    }
    case "series.scan_silent": {
      const thresholdDays = readOptionalPositiveInteger(payload, "thresholdDays") ?? 7;
      return {
        affectedSeriesIds: [],
        thresholdDays,
      };
    }
    default:
      throw {
        code: UNKNOWN_COMMAND,
        message: `unknown rpc path \`${path}\``,
      };
  }
}

function requireNonEmptyString(payload: Record<string, unknown>, key: string): string {
  const raw = payload[key];
  if (typeof raw !== "string" || raw.trim().length === 0) {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` is required and must be a non-empty string`,
    };
  }

  return raw.trim();
}

function readOptionalPositiveInteger(
  payload: Record<string, unknown>,
  key: string,
): number | undefined {
  const raw = payload[key];
  if (raw === undefined || raw === null) {
    return undefined;
  }

  if (typeof raw === "number" && Number.isInteger(raw) && raw > 0) {
    return raw;
  }

  throw {
    code: VALIDATION_ERROR,
    message: `field \`${key}\` must be a positive integer`,
  };
}

function buildErrorEnvelope(
  path: string,
  runtimeStatus: RuntimeStatus,
  code: string,
  message: string,
): RpcEnvelope {
  return {
    ok: false,
    error: {
      code,
      message,
    },
    meta: {
      path,
      runtimeMode: runtimeStatus.mode,
      usedFallback: runtimeStatus.usedFallback,
      respondedAtUnixMs: Date.now(),
    },
  };
}
