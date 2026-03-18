import {
  buildDefaultSilentScanRequest,
  buildDefaultSeriesListRequest,
  loadSeriesList,
  readAdapterSnapshot,
  scanSilentSeries,
} from "../adapter/runtime-adapter";
import { buildInitialShellState } from "./shell-view-model";
import type { RpcError, SeriesCollection, SeriesListData, ShellState } from "./types";

const ARCHIVED_COLLECTION_LIMIT = 200;

export interface SilentAwareSeriesListLoadResult {
  seriesListData: SeriesListData | null;
  seriesListError: RpcError | null;
  silentScanError: RpcError | null;
}

export async function bootstrapShell(): Promise<ShellState> {
  const [snapshot, seriesResult] = await Promise.all([
    readAdapterSnapshot(),
    loadSeriesCollection("active", { refreshSilent: true }),
  ]);

  return buildInitialShellState(
    snapshot,
    seriesResult.seriesListData?.items ?? [],
    seriesResult.seriesListError,
    seriesResult.silentScanError,
  );
}

export async function loadSeriesCollection(
  collection: SeriesCollection,
  options?: {
    query?: string;
    refreshSilent?: boolean;
  },
): Promise<SilentAwareSeriesListLoadResult> {
  const query = options?.query ?? "";
  const refreshSilent = options?.refreshSilent === true && collection === "active" && query.length === 0;
  const silentScanEnvelope = refreshSilent
    ? await scanSilentSeries(buildDefaultSilentScanRequest())
    : null;
  const seriesEnvelope = await loadSeriesList(buildSeriesListRequestForCollection(collection, query));

  return {
    seriesListData:
      seriesEnvelope.ok && seriesEnvelope.data !== undefined
        ? filterSeriesListDataForCollection(collection, seriesEnvelope.data)
        : null,
    seriesListError: seriesEnvelope.ok
      ? null
      : ensureRpcError(seriesEnvelope.error, "failed to load the series list"),
    silentScanError:
      silentScanEnvelope === null || (silentScanEnvelope.ok && silentScanEnvelope.data !== undefined)
        ? null
        : ensureRpcError(silentScanEnvelope.error, "failed to refresh silent series status"),
  };
}

function buildSeriesListRequestForCollection(
  collection: SeriesCollection,
  query: string,
) {
  const baseRequest = buildDefaultSeriesListRequest();

  return {
    ...baseRequest,
    query,
    includeArchived: collection === "archived",
    limit: collection === "archived" ? ARCHIVED_COLLECTION_LIMIT : baseRequest.limit,
  };
}

function filterSeriesListDataForCollection(
  collection: SeriesCollection,
  seriesListData: SeriesListData,
): SeriesListData {
  return {
    ...seriesListData,
    items:
      collection === "archived"
        ? seriesListData.items.filter((item) => item.status === "archived")
        : seriesListData.items.filter((item) => item.status !== "archived"),
  };
}

function ensureRpcError(error: RpcError | undefined, fallbackMessage: string): RpcError {
  if (error !== undefined) {
    return error;
  }

  return {
    code: "INVOKE_FAILED",
    message: fallbackMessage,
  };
}
