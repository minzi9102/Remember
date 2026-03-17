import {
  buildDefaultSeriesListRequest,
  loadSeriesList,
  readAdapterSnapshot,
} from "../adapter/runtime-adapter";
import { buildInitialShellState } from "./shell-view-model";
import type { ShellState } from "./types";

export async function bootstrapShell(): Promise<ShellState> {
  const [snapshot, seriesEnvelope] = await Promise.all([
    readAdapterSnapshot(),
    loadSeriesList(buildDefaultSeriesListRequest()),
  ]);

  return buildInitialShellState(snapshot, seriesEnvelope);
}
