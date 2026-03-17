import type { AdapterSnapshot } from "../adapter/runtime-adapter";
import type {
  CommitItem,
  RpcEnvelope,
  RpcError,
  SeriesListData,
  SeriesSummary,
  ShellState,
} from "./types";

export type ShellAction =
  | {
      type: "series.list.replaced";
      seriesList: SeriesSummary[];
      navigationError: RpcError | null;
    }
  | {
      type: "series.selected";
      seriesId: string;
    }
  | {
      type: "timeline.requested";
      seriesId: string;
    }
  | {
      type: "timeline.loaded";
      seriesId: string;
      items: CommitItem[];
    }
  | {
      type: "timeline.failed";
      seriesId: string;
      error: RpcError;
    }
  | {
      type: "timeline.closed";
    };

export function buildInitialShellState(
  snapshot: AdapterSnapshot,
  seriesEnvelope: RpcEnvelope<SeriesListData>,
): ShellState {
  const seriesList = seriesEnvelope.ok ? seriesEnvelope.data?.items ?? [] : [];

  return {
    appTitle: "Remember",
    subtitle: "Phase 4 Task 2 - List & Timeline Navigation",
    layers: {
      adapter: snapshot.adapter,
      application: "ready",
      repository: snapshot.repository,
    },
    runtimeStatus: snapshot.runtimeStatus,
    commandProbe: snapshot.commandProbe,
    view: "series_list",
    seriesList,
    selectedSeriesId: pickSelectedSeriesId(seriesList, null),
    activeTimelineSeries: null,
    timelineLoadState: "idle",
    timelineItems: [],
    navigationError: seriesEnvelope.ok ? null : ensureRpcError(seriesEnvelope.error, "series list"),
  };
}

export function shellReducer(state: ShellState, action: ShellAction): ShellState {
  switch (action.type) {
    case "series.list.replaced": {
      const nextSelectedSeriesId = pickSelectedSeriesId(action.seriesList, state.selectedSeriesId);
      const activeTimelineSeries = state.activeTimelineSeries
        ? findSeriesById(action.seriesList, state.activeTimelineSeries.id)
        : null;

      if (state.view === "timeline" && activeTimelineSeries === null) {
        return {
          ...state,
          view: "series_list",
          seriesList: action.seriesList,
          selectedSeriesId: nextSelectedSeriesId,
          activeTimelineSeries: null,
          timelineLoadState: "idle",
          timelineItems: [],
          navigationError: action.navigationError,
        };
      }

      return {
        ...state,
        seriesList: action.seriesList,
        selectedSeriesId: nextSelectedSeriesId,
        activeTimelineSeries,
        navigationError: action.navigationError,
      };
    }
    case "series.selected": {
      if (findSeriesById(state.seriesList, action.seriesId) === null) {
        return state;
      }

      return {
        ...state,
        selectedSeriesId: action.seriesId,
      };
    }
    case "timeline.requested": {
      const series = findSeriesById(state.seriesList, action.seriesId);
      if (series === null) {
        return state;
      }

      return {
        ...state,
        view: "timeline",
        selectedSeriesId: series.id,
        activeTimelineSeries: series,
        timelineLoadState: "loading",
        timelineItems: [],
        navigationError: null,
      };
    }
    case "timeline.loaded": {
      if (state.activeTimelineSeries?.id !== action.seriesId) {
        return state;
      }

      return {
        ...state,
        view: "timeline",
        timelineLoadState: "ready",
        timelineItems: action.items,
        navigationError: null,
      };
    }
    case "timeline.failed": {
      if (state.activeTimelineSeries?.id !== action.seriesId) {
        return state;
      }

      return {
        ...state,
        view: "timeline",
        timelineLoadState: "error",
        timelineItems: [],
        navigationError: action.error,
      };
    }
    case "timeline.closed":
      return {
        ...state,
        view: "series_list",
        activeTimelineSeries: null,
        timelineLoadState: "idle",
        timelineItems: [],
        navigationError: null,
      };
    default:
      return state;
  }
}

export function findSeriesById(seriesList: SeriesSummary[], seriesId: string | null): SeriesSummary | null {
  if (seriesId === null) {
    return null;
  }

  return seriesList.find((item) => item.id === seriesId) ?? null;
}

function pickSelectedSeriesId(
  seriesList: SeriesSummary[],
  preferredSeriesId: string | null,
): string | null {
  if (preferredSeriesId !== null && findSeriesById(seriesList, preferredSeriesId) !== null) {
    return preferredSeriesId;
  }

  return seriesList[0]?.id ?? null;
}

function ensureRpcError(error: RpcError | undefined, target: string): RpcError {
  if (error !== undefined) {
    return error;
  }

  return {
    code: "INVOKE_FAILED",
    message: `failed to load ${target}`,
  };
}
