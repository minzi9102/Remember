import { findSeriesById } from "../application/shell-view-model";
import type { CommitItem, ShellState } from "../application/types";

interface RememberShellProps {
  shell: ShellState;
  onSelectSeries: (seriesId: string) => void;
  onOpenTimeline: (seriesId: string) => void;
  onBackToList: () => void;
  onRetryTimeline: () => void;
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
  onSelectSeries,
  onOpenTimeline,
  onBackToList,
  onRetryTimeline,
}: RememberShellProps) {
  const startupSelfHeal = shell.commandProbe.envelope.meta.startupSelfHeal;
  const selectedSeries = findSeriesById(shell.seriesList, shell.selectedSeriesId);

  return (
    <main className="remember-shell" data-testid="remember-shell">
      <header className="shell-header">
        <div>
          <p className="shell-kicker">Low-friction capture</p>
          <h1>{shell.appTitle}</h1>
          <p>{shell.subtitle}</p>
        </div>
        <div className="layer-tags">
          <span>UI: ready</span>
          <span>Adapter: {shell.layers.adapter}</span>
          <span>Application: {shell.layers.application}</span>
          <span>Repository: {shell.layers.repository}</span>
        </div>
      </header>

      <section className="workspace-stage" data-testid="workspace-stage">
        {shell.view === "series_list" ? (
          <article className="panel stage-panel" data-testid="series-list-panel">
            <div className="panel-heading">
              <div>
                <p className="panel-kicker">Level 1</p>
                <h2>Series</h2>
              </div>
              <p className="panel-hint">Single click selects. Double click drills into the timeline.</p>
            </div>

            {shell.navigationError !== null ? (
              <div className="config-warning-banner" data-testid="series-list-error">
                <strong>{shell.navigationError.code}</strong>
                <p>{shell.navigationError.message}</p>
              </div>
            ) : null}

            {shell.seriesList.length === 0 ? (
              <div className="empty-state" data-testid="series-empty-state">
                <h3>No series yet</h3>
                <p>The list will appear here after `series.list` returns data.</p>
              </div>
            ) : (
              <ul className="series-list" aria-label="Series list">
                {shell.seriesList.map((item) => {
                  const isSelected = item.id === shell.selectedSeriesId;

                  return (
                    <li
                      key={item.id}
                      className={`series-row${isSelected ? " is-selected" : ""}`}
                      data-testid={`series-row-${item.id}`}
                    >
                      <button
                        type="button"
                        className="series-select-button"
                        data-testid={`series-select-${item.id}`}
                        onClick={() => onSelectSeries(item.id)}
                        onDoubleClick={() => onOpenTimeline(item.id)}
                      >
                        <span className="series-name">{item.name}</span>
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
          </article>
        ) : (
          <article className="panel stage-panel" data-testid="timeline-panel">
            <div className="panel-heading">
              <div>
                <p className="panel-kicker">Level 2</p>
                <h2>{shell.activeTimelineSeries?.name ?? "Timeline"}</h2>
              </div>
              <button
                type="button"
                className="back-button"
                data-testid="timeline-back-button"
                onClick={onBackToList}
              >
                Back to list
              </button>
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
        )}
      </section>

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

      {shell.view === "series_list" && selectedSeries !== null ? (
        <p className="selection-footer" data-testid="selection-footer">
          Selected series: {selectedSeries.name}
        </p>
      ) : null}
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
