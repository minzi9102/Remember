import type { AdapterSnapshot } from "../adapter/runtime-adapter";
import type {
  CommitItem,
  PendingShellAction,
  RpcError,
  SeriesCollection,
  SeriesSummary,
  ShellState,
} from "./types";

export type ShellAction =
  | {
      type: "series.list.replaced";
      collection: SeriesCollection;
      seriesList: SeriesSummary[];
      navigationError: RpcError | null;
      preferredSeriesId?: string | null;
    }
  | {
      type: "series.selected";
      seriesId: string;
    }
  | {
      type: "series.selection.moved";
      direction: "up" | "down";
    }
  | {
      type: "timeline.requested";
      seriesId: string;
      presentation: "preview" | "focus";
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
    }
  | {
      type: "interaction.search.opened";
    }
  | {
      type: "interaction.search.changed";
      query: string;
    }
  | {
      type: "interaction.create_series.opened";
    }
  | {
      type: "interaction.create_series.changed";
      value: string;
    }
  | {
      type: "interaction.draft_commit.opened";
      initialContent?: string;
    }
  | {
      type: "interaction.draft_commit.changed";
      value: string;
    }
  | {
      type: "interaction.cancelled";
    }
  | {
      type: "interaction.feedback.set";
      feedback: RpcError;
    }
  | {
      type: "interaction.feedback.cleared";
    }
  | {
      type: "interaction.pending.set";
      pendingAction: PendingShellAction | null;
    };

export function buildInitialShellState(
  snapshot: AdapterSnapshot,
  seriesList: SeriesSummary[],
  navigationError: RpcError | null,
  interactionFeedback: RpcError | null = null,
): ShellState {
  return {
    appTitle: "Remember",
    subtitle: "Phase 4 Task 6 - Archived Read-only Timeline",
    layers: {
      adapter: snapshot.adapter,
      application: "ready",
      repository: snapshot.repository,
    },
    runtimeStatus: snapshot.runtimeStatus,
    commandProbe: snapshot.commandProbe,
    view: "series_list",
    seriesCollection: "active",
    seriesList,
    selectedSeriesId: pickSelectedSeriesId(seriesList, null),
    activeSelectedSeriesId: pickSelectedSeriesId(seriesList, null),
    archivedSelectedSeriesId: null,
    activeTimelineSeries: null,
    timelineLoadState: "idle",
    timelineItems: [],
    navigationError,
    interactionMode: "browse",
    searchQuery: "",
    newSeriesNameDraft: "",
    commitDraft: "",
    pendingAction: null,
    interactionFeedback,
  };
}

export function shellReducer(state: ShellState, action: ShellAction): ShellState {
  switch (action.type) {
    case "series.list.replaced": {
      const preferredSeriesId =
        action.preferredSeriesId ?? getStoredSelectedSeriesId(state, action.collection);
      const nextSelectedSeriesId = pickSelectedSeriesId(
        action.seriesList,
        preferredSeriesId,
      );
      const activeTimelineSeries =
        state.seriesCollection === action.collection && state.activeTimelineSeries !== null
          ? findSeriesById(action.seriesList, state.activeTimelineSeries.id)
          : state.activeTimelineSeries;

      if (state.view === "timeline" && activeTimelineSeries === null) {
        return {
          ...state,
          view: "series_list",
          seriesCollection: action.collection,
          seriesList: action.seriesList,
          selectedSeriesId: nextSelectedSeriesId,
          ...applyCollectionSelection(state, action.collection, nextSelectedSeriesId),
          activeTimelineSeries: null,
          timelineLoadState: "idle",
          timelineItems: [],
          navigationError: action.navigationError,
          interactionMode: "browse",
          interactionFeedback: null,
        };
      }

      return {
        ...state,
        seriesCollection: action.collection,
        seriesList: action.seriesList,
        selectedSeriesId: nextSelectedSeriesId,
        ...applyCollectionSelection(state, action.collection, nextSelectedSeriesId),
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
        ...applyCollectionSelection(state, state.seriesCollection, action.seriesId),
        interactionFeedback: null,
      };
    }
    case "series.selection.moved":
      {
        const nextSelectedSeriesId = moveSelectedSeriesId(
          state.seriesList,
          state.selectedSeriesId,
          action.direction,
        );

        return {
          ...state,
          selectedSeriesId: nextSelectedSeriesId,
          ...applyCollectionSelection(state, state.seriesCollection, nextSelectedSeriesId),
          interactionFeedback: null,
        };
      }
    case "timeline.requested": {
      const series = findSeriesById(state.seriesList, action.seriesId);
      if (series === null) {
        return state;
      }

      if (action.presentation === "preview") {
        return {
          ...state,
          view: "series_list",
          selectedSeriesId: series.id,
          ...applyCollectionSelection(state, state.seriesCollection, series.id),
          activeTimelineSeries: series,
          timelineLoadState: "loading",
          timelineItems: [],
          navigationError: null,
          interactionFeedback: null,
        };
      }

      return {
        ...state,
        view: "timeline",
        selectedSeriesId: series.id,
        ...applyCollectionSelection(state, state.seriesCollection, series.id),
        activeTimelineSeries: series,
        timelineLoadState: "loading",
        timelineItems: [],
        navigationError: null,
        interactionMode: "browse",
        searchQuery: "",
        newSeriesNameDraft: "",
        commitDraft: "",
        interactionFeedback: null,
      };
    }
    case "timeline.loaded": {
      if (state.activeTimelineSeries?.id !== action.seriesId) {
        return state;
      }

      return {
        ...state,
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
        interactionMode: "browse",
        searchQuery: "",
        newSeriesNameDraft: "",
        commitDraft: "",
        interactionFeedback: null,
      };
    case "interaction.search.opened":
      return {
        ...state,
        interactionMode: "search",
        searchQuery: "",
        newSeriesNameDraft: "",
        commitDraft: "",
        interactionFeedback: null,
      };
    case "interaction.search.changed":
      return {
        ...state,
        interactionMode: "search",
        searchQuery: action.query,
        interactionFeedback: null,
      };
    case "interaction.create_series.opened":
      return {
        ...state,
        interactionMode: "create_series",
        searchQuery: "",
        newSeriesNameDraft: "",
        commitDraft: "",
        interactionFeedback: null,
      };
    case "interaction.create_series.changed":
      return {
        ...state,
        interactionMode: "create_series",
        newSeriesNameDraft: action.value,
        interactionFeedback: null,
      };
    case "interaction.draft_commit.opened":
      return {
        ...state,
        interactionMode: "draft_commit",
        searchQuery: "",
        newSeriesNameDraft: "",
        commitDraft: action.initialContent ?? "",
        interactionFeedback: null,
      };
    case "interaction.draft_commit.changed":
      return {
        ...state,
        interactionMode: "draft_commit",
        commitDraft: action.value,
        interactionFeedback: null,
      };
    case "interaction.cancelled":
      return {
        ...state,
        interactionMode: "browse",
        searchQuery: "",
        newSeriesNameDraft: "",
        commitDraft: "",
        interactionFeedback: null,
      };
    case "interaction.feedback.set":
      return {
        ...state,
        interactionFeedback: action.feedback,
      };
    case "interaction.feedback.cleared":
      return {
        ...state,
        interactionFeedback: null,
      };
    case "interaction.pending.set":
      return {
        ...state,
        pendingAction: action.pendingAction,
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

function getStoredSelectedSeriesId(
  state: Pick<ShellState, "activeSelectedSeriesId" | "archivedSelectedSeriesId">,
  collection: SeriesCollection,
): string | null {
  return collection === "active" ? state.activeSelectedSeriesId : state.archivedSelectedSeriesId;
}

function applyCollectionSelection(
  state: Pick<ShellState, "activeSelectedSeriesId" | "archivedSelectedSeriesId">,
  collection: SeriesCollection,
  selectedSeriesId: string | null,
): Pick<ShellState, "activeSelectedSeriesId" | "archivedSelectedSeriesId"> {
  if (collection === "active") {
    return {
      activeSelectedSeriesId: selectedSeriesId,
      archivedSelectedSeriesId: state.archivedSelectedSeriesId,
    };
  }

  return {
    activeSelectedSeriesId: state.activeSelectedSeriesId,
    archivedSelectedSeriesId: selectedSeriesId,
  };
}

function moveSelectedSeriesId(
  seriesList: SeriesSummary[],
  selectedSeriesId: string | null,
  direction: "up" | "down",
): string | null {
  if (seriesList.length === 0) {
    return null;
  }

  const currentIndex =
    selectedSeriesId === null ? -1 : seriesList.findIndex((item) => item.id === selectedSeriesId);

  if (currentIndex === -1) {
    return seriesList[0]?.id ?? null;
  }

  const nextIndex =
    direction === "up"
      ? Math.max(0, currentIndex - 1)
      : Math.min(seriesList.length - 1, currentIndex + 1);

  return seriesList[nextIndex]?.id ?? null;
}
