import { startTransition, useEffect, useEffectEvent, useReducer, useRef } from "react";

import {
  appendCommit,
  archiveSeries,
  buildDefaultSeriesListRequest,
  buildDefaultTimelineRequest,
  createSeries,
  loadSeriesList,
  loadTimeline,
} from "./adapter/runtime-adapter";
import { bootstrapShell } from "./application/bootstrap";
import { interpretShellKeyboardEvent } from "./application/shell-shortcuts";
import { shellReducer, type ShellAction } from "./application/shell-view-model";
import type { RpcError, ShellState } from "./application/types";
import { RememberShell, RememberShellLoading } from "./ui/RememberShell";
import "./App.css";

type AppState = ShellState | null;

type AppAction =
  | {
      type: "shell.bootstrap.loaded";
      shell: ShellState;
    }
  | ShellAction;

function appReducer(state: AppState, action: AppAction): AppState {
  if (action.type === "shell.bootstrap.loaded") {
    return action.shell;
  }

  if (state === null) {
    return state;
  }

  return shellReducer(state, action);
}

function App() {
  const [shell, dispatch] = useReducer(appReducer, null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const createSeriesInputRef = useRef<HTMLInputElement | null>(null);
  const commitInputRef = useRef<HTMLInputElement | null>(null);
  const latestSearchRequestIdRef = useRef(0);

  useEffect(() => {
    let isMounted = true;

    bootstrapShell()
      .then((loadedShell) => {
        if (isMounted) {
          startTransition(() => {
            dispatch({ type: "shell.bootstrap.loaded", shell: loadedShell });
          });
        }
      })
      .catch((error) => {
        console.error("[remember][ui] failed to bootstrap shell", error);
      });

    return () => {
      isMounted = false;
    };
  }, []);

  useEffect(() => {
    if (shell === null || shell.view !== "series_list") {
      return;
    }

    const targetInput =
      shell.interactionMode === "search"
        ? searchInputRef.current
        : shell.interactionMode === "create_series"
          ? createSeriesInputRef.current
          : shell.interactionMode === "draft_commit"
            ? commitInputRef.current
            : null;

    if (targetInput !== null) {
      targetInput.focus();
      const end = targetInput.value.length;
      targetInput.setSelectionRange(end, end);
    }
  }, [shell?.interactionMode, shell?.view]);

  const runSeriesSearch = useEffectEvent(async (query: string) => {
    if (shell === null) {
      return;
    }

    const requestId = latestSearchRequestIdRef.current + 1;
    latestSearchRequestIdRef.current = requestId;

    startTransition(() => {
      dispatch({
        type: "interaction.pending.set",
        pendingAction: "search",
      });
    });

    const envelope = await loadSeriesList({
      ...buildDefaultSeriesListRequest(),
      query,
    });

    if (requestId !== latestSearchRequestIdRef.current) {
      return;
    }

    startTransition(() => {
      dispatch({
        type: "interaction.pending.set",
        pendingAction: null,
      });
    });

    if (!envelope.ok || envelope.data === undefined) {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: resolveRpcError(envelope.error, "failed to filter the series list"),
        });
      });
      return;
    }

    const seriesList = envelope.data.items;

    startTransition(() => {
      dispatch({
        type: "series.list.replaced",
        seriesList,
        navigationError: null,
        preferredSeriesId: shell.selectedSeriesId,
      });
    });
  });

  const refreshSeriesList = useEffectEvent(
    async (preferredSeriesId: string | null, feedbackMessage: string) => {
      const envelope = await loadSeriesList(buildDefaultSeriesListRequest());

      if (!envelope.ok || envelope.data === undefined) {
        startTransition(() => {
          dispatch({
            type: "interaction.feedback.set",
            feedback: resolveRpcError(envelope.error, feedbackMessage),
          });
        });
      return false;
    }

    const seriesList = envelope.data.items;

    startTransition(() => {
      dispatch({
        type: "series.list.replaced",
        seriesList,
        navigationError: null,
        preferredSeriesId,
      });
    });

      return true;
    },
  );

  const handleOpenTimeline = useEffectEvent(async (seriesId: string) => {
    startTransition(() => {
      dispatch({ type: "timeline.requested", seriesId });
    });

    const envelope = await loadTimeline(seriesId, buildDefaultTimelineRequest());

    if (!envelope.ok || envelope.data === undefined) {
      startTransition(() => {
        dispatch({
          type: "timeline.failed",
          seriesId,
          error: resolveRpcError(envelope.error, `failed to load timeline for series \`${seriesId}\``),
        });
      });
      return;
    }

    const timelineItems = envelope.data.items;

    startTransition(() => {
      dispatch({
        type: "timeline.loaded",
        seriesId,
        items: timelineItems,
      });
    });
  });

  const submitCreateSeries = useEffectEvent(async () => {
    if (shell === null) {
      return;
    }

    const name = shell.newSeriesNameDraft.trim();
    if (name.length === 0) {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: {
            code: "VALIDATION_ERROR",
            message: "series name is required before pressing Enter",
          },
        });
      });
      return;
    }

    startTransition(() => {
      dispatch({
        type: "interaction.pending.set",
        pendingAction: "create_series",
      });
    });

    const envelope = await createSeries(name);

    startTransition(() => {
      dispatch({
        type: "interaction.pending.set",
        pendingAction: null,
      });
    });

    if (!envelope.ok || envelope.data === undefined) {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: resolveRpcError(envelope.error, "failed to create the series"),
        });
      });
      return;
    }

    const createdSeriesId = envelope.data.series.id;

    startTransition(() => {
      dispatch({ type: "interaction.cancelled" });
    });

    const refreshed = await refreshSeriesList(
      createdSeriesId,
      "series was created, but the list failed to refresh",
    );

    if (!refreshed) {
      return;
    }

    startTransition(() => {
      dispatch({ type: "interaction.draft_commit.opened" });
    });
  });

  const submitCommitDraft = useEffectEvent(async () => {
    if (shell === null) {
      return;
    }

    if (shell.selectedSeriesId === null) {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: {
            code: "NO_SERIES_SELECTED",
            message: "select a series before writing a commit",
          },
        });
      });
      return;
    }

    const content = shell.commitDraft.trim();
    if (content.length === 0) {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: {
            code: "VALIDATION_ERROR",
            message: "commit content is required before pressing Enter",
          },
        });
      });
      return;
    }

    startTransition(() => {
      dispatch({
        type: "interaction.pending.set",
        pendingAction: "append_commit",
      });
    });

    const envelope = await appendCommit(
      shell.selectedSeriesId,
      content,
      new Date().toISOString(),
    );

    startTransition(() => {
      dispatch({
        type: "interaction.pending.set",
        pendingAction: null,
      });
    });

    if (!envelope.ok || envelope.data === undefined) {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: resolveRpcError(envelope.error, "failed to append the commit"),
        });
      });
      return;
    }

    startTransition(() => {
      dispatch({ type: "interaction.cancelled" });
    });

    await refreshSeriesList(
      shell.selectedSeriesId,
      "commit was saved, but the list failed to refresh",
    );
  });

  const archiveSelectedSeries = useEffectEvent(async () => {
    if (shell === null) {
      return;
    }

    if (shell.selectedSeriesId === null) {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: {
            code: "NO_SERIES_SELECTED",
            message: "select a series before archiving",
          },
        });
      });
      return;
    }

    const selectedSeries =
      shell.seriesList.find((item) => item.id === shell.selectedSeriesId) ?? null;
    if (selectedSeries === null || selectedSeries.status !== "silent") {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: {
            code: "ARCHIVE_DISABLED",
            message: "only silent series can be archived with `a`",
          },
        });
      });
      return;
    }

    startTransition(() => {
      dispatch({
        type: "interaction.pending.set",
        pendingAction: "archive_series",
      });
    });

    const envelope = await archiveSeries(shell.selectedSeriesId);

    startTransition(() => {
      dispatch({
        type: "interaction.pending.set",
        pendingAction: null,
      });
    });

    if (!envelope.ok || envelope.data === undefined) {
      startTransition(() => {
        dispatch({
          type: "interaction.feedback.set",
          feedback: resolveRpcError(envelope.error, "failed to archive the selected series"),
        });
      });
      return;
    }

    await refreshSeriesList(
      shell.selectedSeriesId,
      "series was archived, but the list failed to refresh",
    );
  });

  const handleWindowKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (shell === null) {
      return;
    }

    const intent = interpretShellKeyboardEvent(shell, {
      key: event.key,
      shiftKey: event.shiftKey,
      ctrlKey: event.ctrlKey,
      metaKey: event.metaKey,
      altKey: event.altKey,
      repeat: event.repeat,
      isComposing: event.isComposing,
      targetIsEditable: isEditableTarget(event.target),
    });

    if (intent.type === "noop") {
      return;
    }

    event.preventDefault();

    switch (intent.type) {
      case "blocked":
        startTransition(() => {
          dispatch({
            type: "interaction.feedback.set",
            feedback: intent.feedback,
          });
        });
        return;
      case "move_selection":
        startTransition(() => {
          dispatch({
            type: "series.selection.moved",
            direction: intent.direction,
          });
        });
        return;
      case "open_timeline":
        if (shell.selectedSeriesId !== null) {
          void handleOpenTimeline(shell.selectedSeriesId);
        }
        return;
      case "close_timeline":
        startTransition(() => {
          dispatch({ type: "timeline.closed" });
        });
        return;
      case "cancel_interaction":
        startTransition(() => {
          dispatch({ type: "interaction.cancelled" });
        });
        if (shell.interactionMode === "search") {
          void runSeriesSearch("");
        }
        return;
      case "open_search":
        startTransition(() => {
          dispatch({ type: "interaction.search.opened" });
        });
        return;
      case "open_create_series":
        startTransition(() => {
          dispatch({ type: "interaction.create_series.opened" });
        });
        return;
      case "submit_create_series":
        void submitCreateSeries();
        return;
      case "submit_commit":
        void submitCommitDraft();
        return;
      case "archive_selected":
        void archiveSelectedSeries();
        return;
      case "start_commit_draft":
        startTransition(() => {
          dispatch({
            type: "interaction.draft_commit.opened",
            initialContent: intent.initialContent,
          });
        });
        return;
      default:
        return;
    }
  });

  useEffect(() => {
    window.addEventListener("keydown", handleWindowKeyDown);

    return () => {
      window.removeEventListener("keydown", handleWindowKeyDown);
    };
  }, [handleWindowKeyDown]);

  if (shell === null) {
    return <RememberShellLoading />;
  }

  return (
    <RememberShell
      shell={shell}
      searchInputRef={searchInputRef}
      createSeriesInputRef={createSeriesInputRef}
      commitInputRef={commitInputRef}
      onSelectSeries={(seriesId) => {
        startTransition(() => {
          dispatch({ type: "series.selected", seriesId });
        });
      }}
      onOpenTimeline={(seriesId) => {
        void handleOpenTimeline(seriesId);
      }}
      onBackToList={() => {
        startTransition(() => {
          dispatch({ type: "timeline.closed" });
        });
      }}
      onRetryTimeline={() => {
        const activeSeriesId = shell.activeTimelineSeries?.id;
        if (activeSeriesId !== undefined) {
          void handleOpenTimeline(activeSeriesId);
        }
      }}
      onSearchQueryChange={(query) => {
        startTransition(() => {
          dispatch({ type: "interaction.search.changed", query });
        });
        void runSeriesSearch(query);
      }}
      onNewSeriesNameDraftChange={(value) => {
        startTransition(() => {
          dispatch({ type: "interaction.create_series.changed", value });
        });
      }}
      onCommitDraftChange={(value) => {
        startTransition(() => {
          dispatch({ type: "interaction.draft_commit.changed", value });
        });
      }}
    />
  );
}

function resolveRpcError(error: RpcError | undefined, fallbackMessage: string): RpcError {
  if (error !== undefined) {
    return error;
  }

  return {
    code: "INVOKE_FAILED",
    message: fallbackMessage,
  };
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  return (
    target.isContentEditable ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

export default App;
