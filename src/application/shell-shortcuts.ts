import type { RpcError, ShellState } from "./types";

export interface NormalizedShellKeyboardEvent {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  repeat: boolean;
  isComposing: boolean;
  targetIsEditable: boolean;
}

export type ShellKeyboardIntent =
  | {
      type: "noop";
    }
  | {
      type: "blocked";
      feedback: RpcError;
    }
  | {
      type: "move_selection";
      direction: "up" | "down";
    }
  | {
      type: "open_timeline";
    }
  | {
      type: "close_timeline";
    }
  | {
      type: "cancel_interaction";
    }
  | {
      type: "open_search";
    }
  | {
      type: "open_create_series";
    }
  | {
      type: "submit_create_series";
    }
  | {
      type: "submit_commit";
    }
  | {
      type: "archive_selected";
    }
  | {
      type: "start_commit_draft";
      initialContent: string;
    };

const NOOP_INTENT: ShellKeyboardIntent = { type: "noop" };

export function interpretShellKeyboardEvent(
  shell: ShellState,
  event: NormalizedShellKeyboardEvent,
): ShellKeyboardIntent {
  if (event.repeat || event.isComposing || hasBlockedModifier(event)) {
    return NOOP_INTENT;
  }

  if (event.key === "Escape" || event.key === "Esc" || event.key === "ArrowLeft") {
    if (shell.interactionMode !== "browse") {
      return { type: "cancel_interaction" };
    }

    if (shell.view === "timeline") {
      return { type: "close_timeline" };
    }

    return NOOP_INTENT;
  }

  if (shell.interactionMode === "create_series") {
    if (event.key === "Enter") {
      return guardPending(shell, { type: "submit_create_series" });
    }

    return NOOP_INTENT;
  }

  if (shell.interactionMode === "draft_commit") {
    if (event.key === "Enter") {
      return guardPending(shell, { type: "submit_commit" });
    }

    return NOOP_INTENT;
  }

  if (shell.interactionMode === "search") {
    return NOOP_INTENT;
  }

  if (shell.view === "timeline" || event.targetIsEditable) {
    return NOOP_INTENT;
  }

  switch (event.key) {
    case "ArrowUp":
      return shell.seriesList.length > 0
        ? { type: "move_selection", direction: "up" }
        : NOOP_INTENT;
    case "ArrowDown":
      return shell.seriesList.length > 0
        ? { type: "move_selection", direction: "down" }
        : NOOP_INTENT;
    case "ArrowRight":
      return NOOP_INTENT;
    case "/":
      return { type: "open_search" };
    default:
      break;
  }

  const lowerKey = event.key.toLowerCase();

  if (lowerKey === "n" && event.shiftKey) {
    if (shell.seriesCollection === "archived") {
      return buildArchiveReadOnlyIntent();
    }

    return { type: "open_create_series" };
  }

  if (lowerKey === "a" && !event.shiftKey) {
    if (shell.seriesCollection === "archived") {
      return buildArchiveReadOnlyIntent();
    }

    if (shell.selectedSeriesId === null) {
      return buildBlockedIntent("NO_SERIES_SELECTED", "select a series before archiving");
    }

    const selectedSeries = shell.seriesList.find((item) => item.id === shell.selectedSeriesId);
    if (selectedSeries === undefined || selectedSeries.status !== "silent") {
      return buildBlockedIntent("ARCHIVE_DISABLED", "only silent series can be archived with `a`");
    }

    return guardPending(shell, { type: "archive_selected" });
  }

  if (isPrintableKey(event.key)) {
    if (shell.seriesCollection === "archived") {
      return buildArchiveReadOnlyIntent();
    }

    if (shell.selectedSeriesId === null) {
      return buildBlockedIntent("NO_SERIES_SELECTED", "select a series before writing a commit");
    }

    return {
      type: "start_commit_draft",
      initialContent: event.key,
    };
  }

  return NOOP_INTENT;
}

function hasBlockedModifier(event: NormalizedShellKeyboardEvent): boolean {
  return event.ctrlKey || event.metaKey || event.altKey;
}

function isPrintableKey(key: string): boolean {
  return key.length === 1;
}

function guardPending(
  shell: ShellState,
  allowedIntent: Exclude<ShellKeyboardIntent, { type: "noop" } | { type: "blocked" }>,
): ShellKeyboardIntent {
  if (shell.pendingAction === null) {
    return allowedIntent;
  }

  return buildBlockedIntent(
    "ACTION_PENDING",
    `wait for the current ${shell.pendingAction.replace(/_/g, " ")} action to finish`,
  );
}

function buildBlockedIntent(code: string, message: string): ShellKeyboardIntent {
  return {
    type: "blocked",
    feedback: {
      code,
      message,
    },
  };
}

function buildArchiveReadOnlyIntent(): ShellKeyboardIntent {
  return buildBlockedIntent(
    "ARCHIVE_READ_ONLY",
    "archived series are read-only; switch to Active to make changes",
  );
}
