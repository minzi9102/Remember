import type {
  CommitAppendData,
  CommitItem,
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
  SeriesSummary,
  StartupSelfHealSummary,
  TimelineListData,
} from "../application/types";

export interface AdapterSnapshot {
  adapter: LayerState;
  repository: LayerState;
  runtimeStatus: RuntimeStatus;
  commandProbe: CommandProbe;
}

export interface SeriesListRequest {
  query: string;
  includeArchived: boolean;
  cursor: string | null;
  limit: number;
}

export interface TimelineRequest {
  cursor: string | null;
  limit: number;
}

export interface SilentScanRequest {
  now: string;
  thresholdDays: number;
}

interface MockStore {
  series: SeriesSummary[];
  commits: CommitItem[];
  nextCommitOrdinal: number;
  nextTimestampMs: number;
}

const DEFAULT_MODE: RuntimeMode = "sqlite_only";
const RUNTIME_MODE_PATTERN = /\[(sqlite_only|postgres_only|dual_sync)\]/;
const FALLBACK_PATTERN = /\[CONFIG_FALLBACK\]/;
const HOTKEY_DISABLED_PATTERN = /\[HOTKEY_DISABLED\]/;
const DEFAULT_PROBE_PATH = "series.list";
const DEFAULT_MOCK_SESSION = "default";
const VALIDATION_ERROR = "VALIDATION_ERROR";
const NOT_FOUND = "NOT_FOUND";
const CONFLICT = "CONFLICT";
const UNKNOWN_COMMAND = "UNKNOWN_COMMAND";
const PG_TIMEOUT = "PG_TIMEOUT";
const DUAL_WRITE_FAILED = "DUAL_WRITE_FAILED";
const INVOKE_FAILED = "INVOKE_FAILED";
const FORCE_ERROR_CODE_FIELD = "__forceErrorCode";
const MAX_EXCERPT_LENGTH = 48;
const DEFAULT_SILENT_DAYS_THRESHOLD = 7;
const INITIAL_MOCK_TIMESTAMP_MS = Date.parse("2026-03-16T12:00:00Z");
const mockStores = new Map<string, MockStore>();

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
  const hotkeyDisabledMarkExists = HOTKEY_DISABLED_PATTERN.test(title);
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
  if (hotkeyDisabledMarkExists) {
    warnings.push("native runtime reports HOTKEY_DISABLED (global hotkey disabled)");
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
    const search = typeof window === "undefined" ? "" : window.location.search;
    return parseMockRuntimeStatus(search);
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
  const request = buildMockRequest(search, undefined, undefined, runtimeStatus.mode);

  return {
    source: "mock",
    path: request.path,
    envelope: mockInvoke(
      request.path,
      request.payload,
      runtimeStatus,
      request.startupSelfHeal,
      {
        preview: true,
        sessionKey: request.sessionKey,
      },
    ),
  };
}

export function readMockSeriesList(
  search: string,
  request: SeriesListRequest = buildDefaultSeriesListRequest(),
): RpcEnvelope<SeriesListData> {
  const runtimeStatus = parseMockRuntimeStatus(search);
  const mockRequest = buildMockRequest(search, "series.list", { ...request }, runtimeStatus.mode);

  return mockInvoke(
    "series.list",
    mockRequest.payload,
    runtimeStatus,
    mockRequest.startupSelfHeal,
    { sessionKey: mockRequest.sessionKey },
  ) as RpcEnvelope<SeriesListData>;
}

export function readMockCreateSeries(
  search: string,
  name: string,
): RpcEnvelope<SeriesCreateData> {
  const runtimeStatus = parseMockRuntimeStatus(search);
  const mockRequest = buildMockRequest(search, "series.create", { name }, runtimeStatus.mode);

  return mockInvoke(
    "series.create",
    mockRequest.payload,
    runtimeStatus,
    mockRequest.startupSelfHeal,
    { sessionKey: mockRequest.sessionKey },
  ) as RpcEnvelope<SeriesCreateData>;
}

export function readMockTimeline(
  search: string,
  seriesId: string,
  request: TimelineRequest = buildDefaultTimelineRequest(),
): RpcEnvelope<TimelineListData> {
  const runtimeStatus = parseMockRuntimeStatus(search);
  const mockRequest = buildMockRequest(search, "timeline.list", {
    seriesId,
    ...request,
  }, runtimeStatus.mode);

  return mockInvoke(
    "timeline.list",
    mockRequest.payload,
    runtimeStatus,
    mockRequest.startupSelfHeal,
    { sessionKey: mockRequest.sessionKey },
  ) as RpcEnvelope<TimelineListData>;
}

export function readMockAppendCommit(
  search: string,
  seriesId: string,
  content: string,
  clientTs: string,
): RpcEnvelope<CommitAppendData> {
  const runtimeStatus = parseMockRuntimeStatus(search);
  const mockRequest = buildMockRequest(search, "commit.append", {
    seriesId,
    content,
    clientTs,
  }, runtimeStatus.mode);

  return mockInvoke(
    "commit.append",
    mockRequest.payload,
    runtimeStatus,
    mockRequest.startupSelfHeal,
    { sessionKey: mockRequest.sessionKey },
  ) as RpcEnvelope<CommitAppendData>;
}

export function readMockArchiveSeries(
  search: string,
  seriesId: string,
): RpcEnvelope<SeriesArchiveData> {
  const runtimeStatus = parseMockRuntimeStatus(search);
  const mockRequest = buildMockRequest(search, "series.archive", { seriesId }, runtimeStatus.mode);

  return mockInvoke(
    "series.archive",
    mockRequest.payload,
    runtimeStatus,
    mockRequest.startupSelfHeal,
    { sessionKey: mockRequest.sessionKey },
  ) as RpcEnvelope<SeriesArchiveData>;
}

export function readMockScanSilent(
  search: string,
  request: SilentScanRequest = buildDefaultSilentScanRequest(),
): RpcEnvelope<SeriesScanSilentData> {
  const runtimeStatus = parseMockRuntimeStatus(search);
  const mockRequest = buildMockRequest(
    search,
    "series.scan_silent",
    { ...request },
    runtimeStatus.mode,
  );

  return mockInvoke(
    "series.scan_silent",
    mockRequest.payload,
    runtimeStatus,
    mockRequest.startupSelfHeal,
    { sessionKey: mockRequest.sessionKey },
  ) as RpcEnvelope<SeriesScanSilentData>;
}

export async function loadSeriesList(
  request: SeriesListRequest,
): Promise<RpcEnvelope<SeriesListData>> {
  return invokeRpcEnvelope<SeriesListData>("series.list", { ...request });
}

export async function createSeries(name: string): Promise<RpcEnvelope<SeriesCreateData>> {
  return invokeRpcEnvelope<SeriesCreateData>("series.create", { name });
}

export async function loadTimeline(
  seriesId: string,
  request: TimelineRequest,
): Promise<RpcEnvelope<TimelineListData>> {
  return invokeRpcEnvelope<TimelineListData>("timeline.list", {
    seriesId,
    ...request,
  });
}

export async function appendCommit(
  seriesId: string,
  content: string,
  clientTs: string,
): Promise<RpcEnvelope<CommitAppendData>> {
  return invokeRpcEnvelope<CommitAppendData>("commit.append", {
    seriesId,
    content,
    clientTs,
  });
}

export async function archiveSeries(
  seriesId: string,
): Promise<RpcEnvelope<SeriesArchiveData>> {
  return invokeRpcEnvelope<SeriesArchiveData>("series.archive", {
    seriesId,
  });
}

export async function scanSilentSeries(
  request: SilentScanRequest = buildDefaultSilentScanRequest(),
): Promise<RpcEnvelope<SeriesScanSilentData>> {
  return invokeRpcEnvelope<SeriesScanSilentData>("series.scan_silent", { ...request });
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

function buildStartupSelfHealSummary(params: URLSearchParams): StartupSelfHealSummary {
  const completedAt =
    params.get("startup_self_heal_completed_at")?.trim() || new Date().toISOString();

  return {
    scannedAlerts: readNonNegativeIntegerParam(params, "startup_self_heal_scanned") ?? 0,
    repairedAlerts: readNonNegativeIntegerParam(params, "startup_self_heal_repaired") ?? 0,
    unresolvedAlerts: readNonNegativeIntegerParam(params, "startup_self_heal_unresolved") ?? 0,
    failedAlerts: readNonNegativeIntegerParam(params, "startup_self_heal_failed") ?? 0,
    completedAt,
    messages: collectDelimitedParams(
      params,
      "startup_self_heal_message",
      "startup_self_heal_messages",
    ),
  };
}

function collectDelimitedParams(
  params: URLSearchParams,
  repeatedKey: string,
  csvKey: string,
): string[] {
  const values: string[] = [];

  for (const raw of params.getAll(repeatedKey)) {
    const trimmed = raw.trim();
    if (trimmed.length > 0) {
      values.push(trimmed);
    }
  }

  const csv = params.get(csvKey);
  if (csv) {
    values.push(
      ...csv
        .split(/[;,]/)
        .map((item) => item.trim())
        .filter((item) => item.length > 0),
    );
  }

  return [...new Set(values)];
}

function readNonNegativeIntegerParam(
  params: URLSearchParams,
  key: string,
): number | undefined {
  const raw = params.get(key);
  if (raw === null || raw.trim().length === 0) {
    return undefined;
  }

  const value = Number(raw);
  if (!Number.isInteger(value) || value < 0) {
    return undefined;
  }

  return value;
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
  const request = buildMockRequest(search, undefined, undefined, runtimeStatus.mode);

  if (!isTauriRuntime()) {
    return {
      source: "mock",
      path: request.path,
      envelope: mockInvoke(
        request.path,
        request.payload,
        runtimeStatus,
        request.startupSelfHeal,
        {
          preview: true,
          sessionKey: request.sessionKey,
        },
      ),
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
        request.startupSelfHeal,
        INVOKE_FAILED,
        `failed to invoke native rpc shell: ${String(error)}`,
      ),
    };
  }
}

async function invokeRpcEnvelope<T extends RpcData>(
  path: string,
  payload: Record<string, unknown>,
): Promise<RpcEnvelope<T>> {
  const runtimeStatus = await readRuntimeStatus();
  const search = typeof window === "undefined" ? "" : window.location.search;
  const request = buildMockRequest(search, path, payload, runtimeStatus.mode);

  if (!isTauriRuntime()) {
    return mockInvoke(
      path,
      request.payload,
      runtimeStatus,
      request.startupSelfHeal,
      { sessionKey: request.sessionKey },
    ) as RpcEnvelope<T>;
  }

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<RpcEnvelope<T>>("rpc_invoke", {
      path,
      payload,
    });
  } catch (error) {
    return buildErrorEnvelope(
      path,
      runtimeStatus,
      request.startupSelfHeal,
      INVOKE_FAILED,
      `failed to invoke native rpc shell: ${String(error)}`,
    ) as RpcEnvelope<T>;
  }
}

function buildMockRequest(
  search: string,
  forcedPath?: string,
  payloadOverride?: Record<string, unknown>,
  runtimeMode: RuntimeMode = DEFAULT_MODE,
): {
  path: string;
  payload: Record<string, unknown>;
  startupSelfHeal: StartupSelfHealSummary;
  sessionKey: string;
} {
  const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search);
  const path = forcedPath ?? normalizeProbePath(params.get("rpc_path"));
  const forceFail = isTruthy(params.get("rpc_fail"));
  const rawScopedErrorPath = params.get("rpc_error_path")?.trim() ?? null;
  const forceErrorCode = parseForcedErrorCode(
    rawScopedErrorPath === null || rawScopedErrorPath.length === 0 || rawScopedErrorPath === path
      ? params.get("rpc_error")
      : null,
  );
  const basePayload = forceFail
    ? buildFailPayload(path)
    : payloadOverride ?? buildSuccessPayload(path);
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
    startupSelfHeal: buildStartupSelfHealSummary(params),
    sessionKey: buildMockSessionKey(params, runtimeMode),
  };
}

function normalizeProbePath(rawPath: string | null): string {
  if (rawPath === null) {
    return DEFAULT_PROBE_PATH;
  }

  const trimmed = rawPath.trim();
  return trimmed.length > 0 ? trimmed : DEFAULT_PROBE_PATH;
}

export function buildDefaultSeriesListRequest(): SeriesListRequest {
  return {
    query: "",
    includeArchived: false,
    cursor: null,
    limit: 50,
  };
}

export function buildDefaultSilentScanRequest(now = new Date().toISOString()): SilentScanRequest {
  return {
    now,
    thresholdDays: 0,
  };
}

export function buildDefaultTimelineRequest(): TimelineRequest {
  return {
    cursor: null,
    limit: 100,
  };
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
      return { now: "2026-03-16T00:00:00Z", thresholdDays: 0 };
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
  startupSelfHeal: StartupSelfHealSummary,
  options?: {
    preview?: boolean;
    sessionKey?: string;
  },
): RpcEnvelope<RpcData> {
  const meta = {
    path,
    runtimeMode: runtimeStatus.mode,
    usedFallback: runtimeStatus.usedFallback,
    respondedAtUnixMs: Date.now(),
    startupSelfHeal,
  };

  try {
    const data = mockDispatch(
      path,
      payload,
      resolveMockStore(options?.sessionKey ?? buildMockSessionKey(new URLSearchParams(), runtimeStatus.mode), options?.preview ?? false),
    );
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

function mockDispatch(
  path: string,
  payload: Record<string, unknown>,
  store: MockStore,
): RpcData {
  const forcedError = readForcedRpcError(payload);
  if (forcedError !== null) {
    throw forcedError;
  }

  switch (path) {
    case "series.create": {
      const name = requireNonEmptyString(payload, "name");
      const createdAt = nextMockTimestamp(store);
      const data: SeriesCreateData = {
        series: {
          id: buildMockSeriesId(name, store),
          name,
          status: "active",
          lastUpdatedAt: createdAt,
          latestExcerpt: "",
          createdAt,
        },
      };
      store.series.push(cloneSeriesSummary(data.series));
      return data;
    }
    case "series.list": {
      const query = requireString(payload, "query");
      const includeArchived = requireBoolean(payload, "includeArchived");
      const limit = requirePositiveInteger(payload, "limit");
      requireNullableString(payload, "cursor");
      const items = sortSeriesItems(
        store.series.filter((item) => {
          if (!includeArchived && item.status === "archived") {
            return false;
          }

          return query.length === 0 ? true : item.name.toLowerCase().includes(query.toLowerCase());
        }),
      )
        .slice(0, limit)
        .map(cloneSeriesSummary);
      const data: SeriesListData = {
        items,
        nextCursor: null,
        limitEcho: limit,
      };
      return data;
    }
    case "commit.append": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      const content = requireNonEmptyString(payload, "content");
      const createdAt = requireRfc3339String(payload, "clientTs");
      const series = store.series.find((item) => item.id === seriesId);
      if (series === undefined) {
        throw {
          code: NOT_FOUND,
          message: `series \`${seriesId}\` does not exist`,
        };
      }
      if (series.status === "archived") {
        throw {
          code: CONFLICT,
          message: `series \`${seriesId}\` is archived and cannot receive new commits`,
        };
      }

      const commitId = buildMockCommitId(store);
      const latestExcerpt = buildExcerpt(content);
      series.status = "active";
      series.lastUpdatedAt = createdAt;
      series.latestExcerpt = latestExcerpt;
      delete series.archivedAt;
      const commit: CommitItem = {
        id: commitId,
        seriesId,
        content,
        createdAt,
      };
      store.commits.push(commit);
      const data: CommitAppendData = {
        commit: cloneCommitItem(commit),
        series: cloneSeriesSummary(series),
      };
      return data;
    }
    case "timeline.list": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      requireNullableString(payload, "cursor");
      const limit = requirePositiveInteger(payload, "limit");
      const series = store.series.find((item) => item.id === seriesId);
      if (series === undefined) {
        throw {
          code: NOT_FOUND,
          message: `series \`${seriesId}\` does not exist`,
        };
      }
      const data: TimelineListData = {
        seriesId,
        items: sortCommitItems(store.commits.filter((item) => item.seriesId === seriesId))
          .slice(0, limit)
          .map(cloneCommitItem),
        nextCursor: null,
      };
      return data;
    }
    case "series.archive": {
      const seriesId = requireNonEmptyString(payload, "seriesId");
      const series = store.series.find((item) => item.id === seriesId);
      if (series === undefined) {
        throw {
          code: NOT_FOUND,
          message: `series \`${seriesId}\` does not exist`,
        };
      }
      if (series.status !== "archived") {
        const archivedAt = nextMockTimestamp(store);
        series.status = "archived";
        series.archivedAt = archivedAt;
        series.lastUpdatedAt = archivedAt;
      }
      const data: SeriesArchiveData = {
        seriesId,
        archivedAt: series.archivedAt ?? nextMockTimestamp(store),
      };
      return data;
    }
    case "series.scan_silent": {
      const now = requireRfc3339String(payload, "now");
      const thresholdDays = resolveSilentThresholdDays(
        requireNonNegativeInteger(payload, "thresholdDays"),
      );
      const thresholdBefore = computeSilentThresholdBefore(now, thresholdDays);
      const affectedSeriesIds = store.series
        .filter((item) => item.status === "active" && item.lastUpdatedAt < thresholdBefore)
        .map((item) => item.id)
        .sort((left, right) => left.localeCompare(right));

      for (const seriesId of affectedSeriesIds) {
        const series = store.series.find((item) => item.id === seriesId);
        if (series !== undefined) {
          series.status = "silent";
        }
      }

      const data: SeriesScanSilentData = {
        affectedSeriesIds,
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
  if (content.length <= MAX_EXCERPT_LENGTH) {
    return content;
  }

  return `${content.slice(0, MAX_EXCERPT_LENGTH)}...`;
}

function buildMockSessionKey(params: URLSearchParams, runtimeMode: RuntimeMode): string {
  const rawSession = params.get("mock_session")?.trim();
  const sessionId = rawSession && rawSession.length > 0 ? rawSession : DEFAULT_MOCK_SESSION;
  return `${runtimeMode}:${sessionId}`;
}

function resolveMockStore(sessionKey: string, preview: boolean): MockStore {
  const store = getOrCreateMockStore(sessionKey);
  return preview ? cloneMockStore(store) : store;
}

function getOrCreateMockStore(sessionKey: string): MockStore {
  const existing = mockStores.get(sessionKey);
  if (existing !== undefined) {
    return existing;
  }

  const created = createInitialMockStore();
  mockStores.set(sessionKey, created);
  return created;
}

function createInitialMockStore(): MockStore {
  return {
    series: [
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
    commits: [
      {
        id: "stub-commit-001",
        seriesId: "series-inbox",
        content: "first-note",
        createdAt: "2026-03-16T00:00:00Z",
      },
      {
        id: "stub-commit-002",
        seriesId: "series-project-a",
        content: "follow-up-note",
        createdAt: "2026-03-08T09:00:00Z",
      },
      {
        id: "stub-commit-003",
        seriesId: "series-project-a",
        content: "first-project-note",
        createdAt: "2026-03-01T08:30:00Z",
      },
    ],
    nextCommitOrdinal: 4,
    nextTimestampMs: INITIAL_MOCK_TIMESTAMP_MS,
  };
}

function cloneMockStore(store: MockStore): MockStore {
  return {
    series: store.series.map(cloneSeriesSummary),
    commits: store.commits.map(cloneCommitItem),
    nextCommitOrdinal: store.nextCommitOrdinal,
    nextTimestampMs: store.nextTimestampMs,
  };
}

function cloneSeriesSummary(series: SeriesSummary): SeriesSummary {
  return {
    ...series,
  };
}

function cloneCommitItem(commit: CommitItem): CommitItem {
  return {
    ...commit,
  };
}

function sortSeriesItems(items: SeriesSummary[]): SeriesSummary[] {
  return [...items].sort(
    (left, right) =>
      right.lastUpdatedAt.localeCompare(left.lastUpdatedAt) || right.id.localeCompare(left.id),
  );
}

function sortCommitItems(items: CommitItem[]): CommitItem[] {
  return [...items].sort(
    (left, right) =>
      right.createdAt.localeCompare(left.createdAt) || right.id.localeCompare(left.id),
  );
}

function buildMockSeriesId(name: string, store: MockStore): string {
  const slugBase = slugify(name);
  let suffix = 0;

  while (true) {
    const candidate = suffix === 0 ? `stub-series-${slugBase}` : `stub-series-${slugBase}-${suffix + 1}`;
    if (!store.series.some((item) => item.id === candidate)) {
      return candidate;
    }
    suffix += 1;
  }
}

function buildMockCommitId(store: MockStore): string {
  const commitId = `stub-commit-${String(store.nextCommitOrdinal).padStart(3, "0")}`;
  store.nextCommitOrdinal += 1;
  return commitId;
}

function slugify(value: string): string {
  const normalized = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");

  return normalized.length > 0 ? normalized : "series";
}

function nextMockTimestamp(store: MockStore): string {
  const timestamp = new Date(store.nextTimestampMs).toISOString().replace(/\.\d{3}Z$/, "Z");
  store.nextTimestampMs += 1000;
  return timestamp;
}

function resolveSilentThresholdDays(thresholdDays: number): number {
  return thresholdDays === 0 ? DEFAULT_SILENT_DAYS_THRESHOLD : thresholdDays;
}

function computeSilentThresholdBefore(now: string, thresholdDays: number): string {
  const thresholdMs = Date.parse(now) - thresholdDays * 24 * 60 * 60 * 1000;
  return new Date(thresholdMs).toISOString().replace(/\.\d{3}Z$/, "Z");
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

  return new Date(timestamp).toISOString().replace(/\.\d{3}Z$/, "Z");
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

function requireNonNegativeInteger(payload: Record<string, unknown>, key: string): number {
  if (!(key in payload)) {
    throw {
      code: VALIDATION_ERROR,
      message: `field \`${key}\` must be a non-negative integer`,
    };
  }

  const raw = payload[key];
  if (typeof raw === "number" && Number.isInteger(raw) && raw >= 0) {
    return raw;
  }

  throw {
    code: VALIDATION_ERROR,
    message: `field \`${key}\` must be a non-negative integer`,
  };
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
  startupSelfHeal: StartupSelfHealSummary,
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
      startupSelfHeal,
    },
  };
}
