import type { ShellState } from "../application/types";

interface RememberShellProps {
  shell: ShellState;
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

export function RememberShell({ shell }: RememberShellProps) {
  return (
    <main className="remember-shell" data-testid="remember-shell">
      <header className="shell-header">
        <h1>{shell.appTitle}</h1>
        <p>{shell.subtitle}</p>
        <div className="layer-tags">
          <span>UI: ready</span>
          <span>Adapter: {shell.layers.adapter}</span>
          <span>Application: {shell.layers.application}</span>
          <span>Repository: {shell.layers.repository}</span>
        </div>
      </header>

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
      </section>

      <section className="shell-grid">
        <article className="panel">
          <h2>Series List Preview</h2>
          <ul>
            {shell.seriesPreview.map((item) => (
              <li key={item.id}>
                <div className="title">{item.name}</div>
                <div className="meta">{item.latestExcerpt}</div>
              </li>
            ))}
          </ul>
        </article>

        <article className="panel">
          <h2>Timeline Preview</h2>
          <ul>
            {shell.timelinePreview.map((item) => (
              <li key={`${item.createdAt}-${item.content}`}>
                <div className="title">{item.createdAt}</div>
                <div className="meta">{item.content}</div>
              </li>
            ))}
          </ul>
        </article>
      </section>
    </main>
  );
}
