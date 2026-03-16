import type {
  CommitAppendData,
  CommandProbe,
  LayerState,
  RpcData,
  RpcEnvelope,
  RuntimeMode,
  RuntimeStatus,
  SeriesArchiveData,
  SeriesCreateData,
  SeriesListData,
  SeriesScanSilentData,
  TimelineListData,
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
const PG_TIMEOUT = "PG_TIMEOUT";
const DUAL_WRITE_FAILED = "DUAL_WRITE_FAILED";
const INVOKE_FAILED = "INVOKE_FAILED";
const FORCE_ERROR_CODE_FIELD = "__forceErrorCode";

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
  const forceErrorCode = parseForcedErrorCode(params.get("rpc_error"));
  const basePayload = forceFail ? buildFailPayload(path) : buildSuccessPayload(path);
  const payload =
    forceErrorCode === null
      ? basePayload
      : {
          ...basePayload,
          [FORCE_ERROR_CODE_FIELD]: forceErrorCode,
        };

  return {
    path,
    payload,
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

function parseForcedErrorCode(raw: string | null): string | null {
  if (raw === null) {
    return null;
  }

  const normalized = raw.trim().toLowerCase();
  switch (normalized) {
    case "pg_timeout":
      return PG_TIMEOUT;
    case "dual_write_failed":
      return DUAL_WRITE_FAILED;
    case "validation_error":
      return VALIDATION_ERROR;
    default:
      return null;
  }
}

function buildSuccessPayload(path: string): Record<string, unknown> {
  switch (path) {
    case "series.create":
      return { name: "Inbox" };
    case "series.list":
      return { query: "", includeArchived: false, cursor: null, limit: 50 };
    case "commit.append":
      return {
        seriesId: "series-inbox",
        content: "first-note",
        clientTs: "2026-03-16T00:00:00Z",
      };
    case "timeline.list":
      return { seriesId: "series-inbox", cursor: null, limit: 20 };
    case "series.archive":
      return { seriesId: "series-inbox" };
    case "series.scan_silent":
      return { now: "2026-03-16T00:00:00Z", thresholdDays: 7 };
    default:
      return {};
  }
}

function buildFailPayload(path: string): Record<string, unknown> {
  switch (path) {
    case "series.create":
      return { name: "" };
    case "series.list":
      return { query: "", includeArchived: false, cursor: null, limit: 0 };
    case "commit.append":
      return { seriesId: "", content: "", clientTs: "invalid-timestamp" };
    case "timeline.list":
      return { seriesId: "", cursor: null, limit: 20 };
    case "series.archive":
      return { seriesId: "" };
    case "series.scan_silent":
      return { now: "invalid-timestamp", thresholdDays: 0 };
    default:
      return {};
  }
}

function mockInvoke(
  path: string,
  payload: Record<string, unknown>,
  runtimeStatus: RuntimeStatus,
): RpcEnvelope<RpcData> {
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

function mockDispatch(path: string, payload: Record<string, unknown>): RpcData {
  const forcedError = readForcedRpcError(payload);
  if (forcedError !== null) {
    throw forcedError;
  }

  switch (path) {
    case "series.create": {
      const name = requireNonEmptyString(payload, "name");
      const data: SeriesCreateData = {
        series: {
          id: "stub-series-inbox",
          name,
          status: "active",
          lastUpdatedAt: "2026-03-16T00:00:00Z",
          latestExcerpt: "stubbed-command-shell",
          createdAt: "2026-03-15T00:00:00Z",
        },
      };
      return data;
    }
    case "series.list": {
      const query = requireString(payload, "query");
      const includeArchived = requireBoolean(payload, "includeArchived");
      const cursor = requireNullableString(payload, "cursor");
      const limit = requirePositiveInteger(payload, "limit");
      const data: SeriesListData = {
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
        nextCursor: query.length > 0 ? null : cursor,
        limitEcho: limit,
      };
      void includeArchived;
      return data;
    }
    case "commit.append": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      const content = requireNonEmptyString(payload, "content");
      requireRfc3339String(payload, "clientTs");
      const data: CommitAppendData = {
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
          latestExcerpt: buildExcerpt(content),
          createdAt: "2026-03-15T00:00:00Z",
        },
      };
      return data;
    }
    case "timeline.list": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      requireNullableString(payload, "cursor");
      requirePositiveInteger(payload, "limit");
      const data: TimelineListData = {
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
      return data;
    }
    case "series.archive": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      const data: SeriesArchiveData = {
        seriesId,
        archivedAt: "2026-03-16T00:00:00Z",
      };
      return data;
    }
    case "series.scan_silent": {
      requireRfc3339String(payload, "now");
      const thresholdDays = requirePositiveInteger(payload, "thresholdDays");
      const data: SeriesScanSilentData = {
        affectedSeriesIds: [],
        thresholdDays,
      };
      return data;
    }
    default:
      throw {
        code: UNKNOWN_COMMAND,
        message: `unknown rpc path \`${path}\``,
      };
  }
}

function buildExcerpt(content: string): string {
  if (content.length <= 48) {
    return content;
  }

  return `${content.slice(0, 48)}...`;
}

function readForcedRpcError(payload: Record<string, unknown>): { code: string; message: string } | null {
  const raw = payload[FORCE_ERROR_CODE_FIELD];
  if (typeof raw !== "string") {
    return null;
  }

  const normalized = raw.trim().toUpperCase();
  switch (normalized) {
    case PG_TIMEOUT:
      return {
        code: PG_TIMEOUT,
        message: "simulated postgres timeout for diagnostics",
      };
    case DUAL_WRITE_FAILED:
      return {
        code: DUAL_WRITE_FAILED,
        message: "simulated dual write failure for diagnostics",
      };
    case VALIDATION_ERROR:
      return {
        code: VALIDATION_ERROR,
        message: "simulated validation error for diagnostics",
      };
    default:
      return {
        code: VALIDATION_ERROR,
        message: `field \`${FORCE_ERROR_CODE_FIELD}\` must be one of ${PG_TIMEOUT}, ${DUAL_WRITE_FAILED}, ${VALIDATION_ERROR}`,
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

function requireString(payload: Record<string, unknown>, key: string): string {
  const raw = payload[key];
  if (typeof raw !== "string") {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` is required and must be a string`,
    };
  }

  return raw;
}

function requireBoolean(payload: Record<string, unknown>, key: string): boolean {
  const raw = payload[key];
  if (typeof raw !== "boolean") {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` is required and must be a boolean`,
    };
  }

  return raw;
}

function requireNullableString(payload: Record<string, unknown>, key: string): string | null {
  if (!(key in payload)) {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` is required and must be a string or null`,
    };
  }

  const raw = payload[key];
  if (raw === null) {
    return null;
  }

  if (typeof raw !== "string") {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` is required and must be a string or null`,
    };
  }

  return raw.trim().length > 0 ? raw.trim() : null;
}

function requireRfc3339String(payload: Record<string, unknown>, key: string): string {
  const raw = requireNonEmptyString(payload, key);
  const timestamp = Date.parse(raw);
  if (Number.isNaN(timestamp)) {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` must be a valid RFC3339 timestamp`,
    };
  }

  return raw;
}

function requirePositiveInteger(payload: Record<string, unknown>, key: string): number {
  if (!(key in payload)) {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` must be a positive integer`,
    };
  }

  const value = readOptionalPositiveInteger(payload, key);
  if (value === undefined) {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` must be a positive integer`,
    };
  }

  return value;
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
