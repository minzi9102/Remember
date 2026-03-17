import { startTransition, useEffect, useReducer } from "react";

import { buildDefaultTimelineRequest, loadTimeline } from "./adapter/runtime-adapter";
import { bootstrapShell } from "./application/bootstrap";
import { shellReducer, type ShellAction } from "./application/shell-view-model";
import type { ShellState } from "./application/types";
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

  if (shell === null) {
    return <RememberShellLoading />;
  }

  async function handleOpenTimeline(seriesId: string) {
    startTransition(() => {
      dispatch({ type: "timeline.requested", seriesId });
    });

    const envelope = await loadTimeline(seriesId, buildDefaultTimelineRequest());

    if (!envelope.ok || envelope.data === undefined) {
      startTransition(() => {
        dispatch({
          type: "timeline.failed",
          seriesId,
          error: envelope.error ?? {
            code: "INVOKE_FAILED",
            message: `failed to load timeline for series \`${seriesId}\``,
          },
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
  }

  return (
    <RememberShell
      shell={shell}
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
    />
  );
}

export default App;
