import {
  buildDefaultSilentScanRequest,
  buildDefaultSeriesListRequest,
  loadSeriesList,
  readAdapterSnapshot,
  scanSilentSeries,
} from "../adapter/runtime-adapter";
import { buildInitialShellState } from "./shell-view-model";
import type { RpcError, SeriesListData, ShellState } from "./types";

export interface SilentAwareSeriesListLoadResult {
  seriesListData: SeriesListData | null;
  seriesListError: RpcError | null;
  silentScanError: RpcError | null;
}

export async function bootstrapShell(): Promise<ShellState> {
  const [snapshot, seriesResult] = await Promise.all([
    readAdapterSnapshot(),
    loadSilentAwareSeriesList(),
  ]);

  return buildInitialShellState(
    snapshot,
    seriesResult.seriesListData?.items ?? [],
    seriesResult.seriesListError,
    seriesResult.silentScanError,
  );
}

export async function loadSilentAwareSeriesList(): Promise<SilentAwareSeriesListLoadResult> {
  const silentScanEnvelope = await scanSilentSeries(buildDefaultSilentScanRequest());
  const seriesEnvelope = await loadSeriesList(buildDefaultSeriesListRequest());

  return {
    seriesListData: seriesEnvelope.ok ? seriesEnvelope.data ?? null : null,
    seriesListError: seriesEnvelope.ok
      ? null
      : ensureRpcError(seriesEnvelope.error, "failed to load the series list"),
    silentScanError:
      silentScanEnvelope.ok && silentScanEnvelope.data !== undefined
        ? null
        : ensureRpcError(silentScanEnvelope.error, "failed to refresh silent series status"),
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
