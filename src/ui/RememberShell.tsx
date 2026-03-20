import { useEffect, useRef, type RefObject } from "react";

import { findSeriesById } from "../application/shell-view-model";
import type { CommitItem, SeriesCollection, ShellState } from "../application/types";

interface RememberShellProps {
  shell: ShellState;
  isDiagnosticsDrawerOpen: boolean;
  onToggleDiagnosticsDrawer: () => void;
  searchInputRef?: RefObject<HTMLInputElement | null>;
  createSeriesInputRef?: RefObject<HTMLInputElement | null>;
  commitInputRef?: RefObject<HTMLInputElement | null>;
  onSelectCollection: (collection: SeriesCollection) => void;
  onSelectSeries: (seriesId: string) => void;
  onOpenTimeline: (seriesId: string) => void;
  onBackToList: () => void;
  onRetryTimeline: () => void;
  onSearchQueryChange: (query: string) => void;
  onNewSeriesNameDraftChange: (value: string) => void;
  onCommitDraftChange: (value: string) => void;
}

export function RememberShellLoading() {
  return (
    <main className="remember-shell remember-shell-loading" data-testid="remember-shell-loading">
      <header className="shell-header">
        <h1>Remember</h1>
        <p data-testid="runtime-loading-text">Loading runtime diagnostics...</p>
      </header>
    </main>
  );
}

export function RememberShell({
  shell,
  isDiagnosticsDrawerOpen,
  onToggleDiagnosticsDrawer,
  searchInputRef,
  createSeriesInputRef,
  commitInputRef,
  onSelectCollection,
  onSelectSeries,
  onOpenTimeline,
  onBackToList,
  onRetryTimeline,
  onSearchQueryChange,
  onNewSeriesNameDraftChange,
  onCommitDraftChange,
}: RememberShellProps) {
  const startupSelfHeal = shell.commandProbe.envelope.meta.startupSelfHeal;
  const selectedSeries = findSeriesById(shell.seriesList, shell.selectedSeriesId);
  const isArchivedCollection = shell.seriesCollection === "archived";
  const timelineIsArchived = shell.activeTimelineSeries?.status === "archived";
  const listHint = isArchivedCollection
    ? "`↑/↓` select, `→` opens timeline, `/` searches. Archived series stay read-only."
    : "`↑/↓` select, `→` opens timeline, `/` searches, `Shift+N` creates, `a` archives silent, type to capture.";
  const selectedSeriesCardRef = useRef<HTMLLIElement | null>(null);
  const mainRailRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (shell.selectedSeriesId === null || shell.seriesList.length === 0) {
      return;
    }

    selectedSeriesCardRef.current?.scrollIntoView({
      behavior: "smooth",
      block: "nearest",
      inline: "nearest",
    });
  }, [shell.selectedSeriesId, shell.seriesCollection, shell.seriesList]);

  useEffect(() => {
    const mainRailElement = mainRailRef.current;
    if (mainRailElement === null) {
      return;
    }

    const handleWheel = (event: WheelEvent) => {
      if (
        event.ctrlKey ||
        event.deltaY === 0 ||
        Math.abs(event.deltaX) > Math.abs(event.deltaY)
      ) {
        return;
      }

      if (event.cancelable) {
        event.preventDefault();
      }

      const normalizedDelta =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? event.deltaY * 16
          : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
            ? event.deltaY * mainRailElement.clientWidth
            : event.deltaY;

      mainRailElement.scrollLeft += normalizedDelta;
    };

    mainRailElement.addEventListener("wheel", handleWheel, {
      passive: false,
    });

    return () => {
      mainRailElement.removeEventListener("wheel", handleWheel);
    };
  }, []);

  return (
    <main className="remember-shell" data-testid="remember-shell">
      <section className="top-dock main-rail-wrapper" data-testid="top-dock">
        <article className="panel stage-panel cross-axis-main top-dock-panel" data-testid="series-list-panel">
          {shell.navigationError !== null ? (
            <div className="config-warning-banner" data-testid="series-list-error">
              <strong>{shell.navigationError.code}</strong>
              <p>{shell.navigationError.message}</p>
            </div>
          ) : null}

          {shell.interactionFeedback !== null ? (
            <div className="config-warning-banner interaction-feedback" data-testid="interaction-feedback-banner">
              <strong>{shell.interactionFeedback.code}</strong>
              <p>{shell.interactionFeedback.message}</p>
            </div>
          ) : null}

          {shell.view === "series_list" && shell.interactionMode === "search" ? (
            <div className="command-surface command-surface-global" data-testid="search-command-bar">
              <label className="command-label" htmlFor="series-search-input">
                Search series
              </label>
              <input
                id="series-search-input"
                ref={searchInputRef}
                className="command-input"
                data-testid="search-command-input"
                type="text"
                value={shell.searchQuery}
                onChange={(event) => onSearchQueryChange(event.target.value)}
                placeholder="Type to filter series names"
                autoComplete="off"
                spellCheck={false}
              />
              <p className="command-help">
                Esc closes search and restores the full list.
                {shell.pendingAction === "search" ? " Searching..." : ""}
              </p>
            </div>
          ) : null}

          {shell.view === "series_list" && !isArchivedCollection && shell.interactionMode === "create_series" ? (
            <div className="command-surface command-surface-global" data-testid="create-series-command-bar">
              <label className="command-label" htmlFor="series-create-input">
                Create a new series
              </label>
              <input
                id="series-create-input"
                ref={createSeriesInputRef}
                className="command-input"
                data-testid="create-series-command-input"
                type="text"
                value={shell.newSeriesNameDraft}
                onChange={(event) => onNewSeriesNameDraftChange(event.target.value)}
                placeholder="Series name"
                autoComplete="off"
                spellCheck={false}
                disabled={shell.pendingAction === "create_series"}
              />
              <p className="command-help">
                Enter creates the series. Esc cancels.
                {shell.pendingAction === "create_series" ? " Creating..." : ""}
              </p>
            </div>
          ) : null}

          <div className="main-rail" data-testid="main-rail" ref={mainRailRef}>
            {shell.seriesList.length === 0 ? (
              <div className="empty-state" data-testid="series-empty-state">
                <h3>{isArchivedCollection ? "No archived series" : "No series yet"}</h3>
                <p>
                  {isArchivedCollection
                    ? "Archived series appear here after you press `a` on a silent series."
                    : "The list will appear here after `series.list` returns data."}
                </p>
              </div>
            ) : (
              <ul className="series-rail" aria-label="Series list" data-testid="series-rail">
                {shell.seriesList.map((item) => {
                  const isSelected = item.id === shell.selectedSeriesId;
                  const isSilent = item.status === "silent";
                  const isArchived = item.status === "archived";

                  return (
                    <li
                      key={item.id}
                      ref={isSelected ? selectedSeriesCardRef : null}
                      className={`series-card${isSelected ? " is-selected" : ""}${isSilent ? " is-silent" : ""}${isArchived ? " is-archived" : ""}`}
                      data-testid={`series-row-${item.id}`}
                    >
                      <button
                        type="button"
                        className="series-select-button"
                        data-testid={`series-select-${item.id}`}
                        onClick={() => onSelectSeries(item.id)}
                        onDoubleClick={() => onOpenTimeline(item.id)}
                      >
                        <span className="series-nameplate">
                          <span className="series-name">{item.name}</span>
                          {isSilent ? (
                            <span className="series-status-badge" data-testid={`series-status-${item.id}`}>
                              Silent
                            </span>
                          ) : null}
                          {isArchived ? (
                            <span className="series-status-badge archived" data-testid={`series-status-${item.id}`}>
                              Archived
                            </span>
                          ) : null}
                        </span>
                        <span className="series-excerpt">{item.latestExcerpt}</span>
                      </button>
                      {isSelected ? (
                        <button
                          type="button"
                          className="series-open-button"
                          data-testid={`series-open-${item.id}`}
                          onClick={() => onOpenTimeline(item.id)}
                        >
                          View timeline
                        </button>
                      ) : null}
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          {shell.view === "series_list" && !isArchivedCollection && shell.interactionMode === "draft_commit" ? (
            <div className="command-surface command-surface-compose" data-testid="commit-draft-command-bar">
              <label className="command-label" htmlFor="commit-draft-input">
                Append commit to {selectedSeries?.name ?? "the selected series"}
              </label>
              <input
                id="commit-draft-input"
                ref={commitInputRef}
                className="command-input"
                data-testid="commit-draft-command-input"
                type="text"
                value={shell.commitDraft}
                onChange={(event) => onCommitDraftChange(event.target.value)}
                placeholder="Type a commit and press Enter"
                autoComplete="off"
                spellCheck={false}
                disabled={shell.pendingAction === "append_commit"}
              />
              <p className="command-help">
                Enter submits the commit. Esc cancels.
                {shell.pendingAction === "append_commit" ? " Saving..." : ""}
              </p>
            </div>
          ) : null}

          {shell.pendingAction === "archive_series" ? (
            <p className="command-status" data-testid="archive-pending-status">
              Archiving the selected silent series...
            </p>
          ) : null}
        </article>
      </section>

      <section className="workspace-stage cross-axis-stage" data-testid="workspace-stage">
        {shell.view === "series_list" ? (
          <article
            className="workspace-glass-placeholder"
            data-testid="workspace-glass-placeholder"
            aria-hidden="true"
          />
        ) : null}

        {shell.view === "timeline" ? (
          <article className="panel stage-panel timeline-lane" data-testid="timeline-lane">
            <div className="panel-heading">
              <div>
                <p className="panel-kicker">Timeline Lane</p>
                <div className="timeline-title-row">
                  <h2>{shell.activeTimelineSeries?.name ?? "Timeline"}</h2>
                  {timelineIsArchived ? (
                    <span className="series-status-badge archived" data-testid="timeline-archived-badge">
                      Archived
                    </span>
                  ) : null}
                </div>
              </div>
              <div className="timeline-heading-actions">
                <p className="panel-hint timeline-hint">
                  {timelineIsArchived
                    ? "Archived timeline is read-only. `←` or `Esc` returns."
                    : "Read-only timeline. `←` or `Esc` returns."}
                </p>
                <button
                  type="button"
                  className="back-button"
                  data-testid="timeline-back-button"
                  onClick={onBackToList}
                >
                  Back to list
                </button>
              </div>
            </div>

            {shell.timelineLoadState === "loading" ? (
              <div className="empty-state" data-testid="timeline-loading-state">
                <h3>Loading timeline</h3>
                <p>Fetching commits for the selected series.</p>
              </div>
            ) : null}

            {shell.timelineLoadState === "error" && shell.navigationError !== null ? (
              <div className="config-warning-banner timeline-error" data-testid="timeline-error-state">
                <strong>{shell.navigationError.code}</strong>
                <p>{shell.navigationError.message}</p>
                <div className="timeline-error-actions">
                  <button type="button" className="series-open-button" onClick={onRetryTimeline}>
                    Retry
                  </button>
                  <button type="button" className="back-button ghost" onClick={onBackToList}>
                    Return
                  </button>
                </div>
              </div>
            ) : null}

            {shell.timelineLoadState === "ready" && shell.timelineItems.length === 0 ? (
              <div className="empty-state" data-testid="timeline-empty-state">
                <h3>No commits yet</h3>
                <p>This timeline is read-only and currently has nothing to show.</p>
              </div>
            ) : null}

            {shell.timelineLoadState === "ready" && shell.timelineItems.length > 0 ? (
              <ol className="timeline-list" data-testid="timeline-list">
                {shell.timelineItems.map((item) => (
                  <li key={item.id} className="timeline-entry">
                    <TimelineEntry item={item} />
                  </li>
                ))}
              </ol>
            ) : null}
          </article>
        ) : null}
      </section>

      <div className="floating-corner-controls" data-testid="floating-corner-controls">
        <button
          type="button"
          className="diagnostics-toggle-button diagnostics-toggle-button-mini"
          data-testid="diagnostics-drawer-toggle"
          aria-expanded={isDiagnosticsDrawerOpen}
          aria-controls="diagnostics-drawer-panel"
          onClick={onToggleDiagnosticsDrawer}
        >
          Diag
        </button>
        <div className="view-toggle-container" data-testid="view-toggle-container">
          <div className="collection-toggle" data-testid="series-collection-toggle">
            <button
              type="button"
              className={`collection-toggle-button${!isArchivedCollection ? " is-active" : ""}`}
              data-testid="series-collection-active-button"
              aria-pressed={!isArchivedCollection}
              onClick={() => onSelectCollection("active")}
            >
              Active
            </button>
            <button
              type="button"
              className={`collection-toggle-button${isArchivedCollection ? " is-active" : ""}`}
              data-testid="series-collection-archived-button"
              aria-pressed={isArchivedCollection}
              onClick={() => onSelectCollection("archived")}
            >
              Archived
            </button>
          </div>
        </div>
      </div>

      <p className="shortcut-hints-watermark" data-testid="shortcut-hints-watermark">
        {listHint}
      </p>

      <aside
        id="diagnostics-drawer-panel"
        className={`diagnostics-drawer${isDiagnosticsDrawerOpen ? " is-open" : ""}`}
        data-testid="diagnostics-drawer"
        aria-hidden={!isDiagnosticsDrawerOpen}
      >
        <div className="diagnostics-drawer-header">
          <h2>Runtime Diagnostics</h2>
          <button type="button" className="back-button ghost" onClick={onToggleDiagnosticsDrawer}>
            Close
          </button>
        </div>
        <section className="diagnostics-grid">
          <section className="runtime-diagnostics panel" data-testid="runtime-diagnostics">
            <h2>Runtime Mode</h2>
            <div className="runtime-tags">
              <span className="runtime-tag mode" data-testid="runtime-mode-badge">
                mode: {shell.runtimeStatus.mode}
              </span>
              <span className="runtime-tag source" data-testid="runtime-source-badge">
                source: {shell.runtimeStatus.source}
              </span>
              <span className="runtime-tag fallback" data-testid="runtime-fallback-badge">
                fallback: {shell.runtimeStatus.usedFallback ? "on" : "off"}
              </span>
            </div>

            {shell.runtimeStatus.warnings.length > 0 ? (
              <div className="config-warning-banner" data-testid="config-warning-banner">
                <strong>Config warning</strong>
                <ul>
                  {shell.runtimeStatus.warnings.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              </div>
            ) : (
              <p className="config-ok" data-testid="config-ok-banner">
                No runtime warnings.
              </p>
            )}
          </section>

          <section className="command-diagnostics panel" data-testid="command-envelope-panel">
            <h2>Command Envelope</h2>
            <div className="runtime-tags">
              <span className="runtime-tag mode" data-testid="command-envelope-path">
                path: {shell.commandProbe.path}
              </span>
              <span className="runtime-tag source" data-testid="command-envelope-source">
                source: {shell.commandProbe.source}
              </span>
              <span className="runtime-tag fallback" data-testid="command-envelope-ok">
                ok: {shell.commandProbe.envelope.ok ? "true" : "false"}
              </span>
            </div>

            {shell.commandProbe.envelope.ok ? (
              <p className="config-ok" data-testid="command-envelope-success">
                envelope success with data payload.
              </p>
            ) : (
              <div className="config-warning-banner" data-testid="command-envelope-error">
                <strong>Command error</strong>
                <p data-testid="command-envelope-error-code">
                  code: {shell.commandProbe.envelope.error?.code ?? "UNKNOWN"}
                </p>
                <p>{shell.commandProbe.envelope.error?.message ?? "No error message."}</p>
              </div>
            )}

            <pre className="command-meta" data-testid="command-envelope-meta">
              {JSON.stringify(shell.commandProbe.envelope.meta, null, 2)}
            </pre>
            <pre className="command-meta" data-testid="command-envelope-data">
              {JSON.stringify(shell.commandProbe.envelope.data ?? null, null, 2)}
            </pre>
          </section>

          <section className="startup-self-heal panel" data-testid="startup-self-heal-panel">
            <h2>Startup Self-Heal</h2>
            <div className="runtime-tags">
              <span className="runtime-tag mode" data-testid="startup-self-heal-scanned">
                scanned: {startupSelfHeal.scannedAlerts}
              </span>
              <span className="runtime-tag source" data-testid="startup-self-heal-repaired">
                repaired: {startupSelfHeal.repairedAlerts}
              </span>
              <span className="runtime-tag fallback" data-testid="startup-self-heal-unresolved">
                unresolved: {startupSelfHeal.unresolvedAlerts}
              </span>
              <span className="runtime-tag fallback" data-testid="startup-self-heal-failed">
                failed: {startupSelfHeal.failedAlerts}
              </span>
            </div>
            <p data-testid="startup-self-heal-completed-at">
              completed at: {startupSelfHeal.completedAt}
            </p>

            {startupSelfHeal.unresolvedAlerts > 0 && startupSelfHeal.messages.length > 0 ? (
              <div className="config-warning-banner" data-testid="startup-self-heal-messages">
                <strong>Unresolved startup alerts</strong>
                <ul>
                  {startupSelfHeal.messages.map((message) => (
                    <li key={message}>{message}</li>
                  ))}
                </ul>
              </div>
            ) : (
              <p className="config-ok" data-testid="startup-self-heal-clean">
                No unresolved startup alerts.
              </p>
            )}
          </section>
        </section>
      </aside>
    </main>
  );
}

function TimelineEntry({ item }: { item: CommitItem }) {
  return (
    <>
      <p className="timeline-entry-date">{item.createdAt}</p>
      <p className="timeline-entry-content">{item.content}</p>
    </>
  );
}
