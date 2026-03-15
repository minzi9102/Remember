import type { ShellState } from "../application/types";

interface RememberShellProps {
  shell: ShellState;
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
