// Two-pane review shell. Left: file list. Right: raw diff text.
// Visual target for GPUI/Tauri parity.

import type { ReviewConfig } from '../lib/review-state'
import { useReview } from '../lib/review-state'

export function ReviewLayout(props: { config: ReviewConfig }) {
  const r = useReview(props.config)

  return (
    <div className="lgtm-layout">
      <aside className="lgtm-sidebar">
        <header className="lgtm-sidebar-header">
          <span>Changed files</span>
          <button type="button" onClick={r.reload} className="lgtm-reload">
            Reload
          </button>
        </header>
        <ul className="lgtm-filelist">
          {r.files.map((file) => {
            const active = r.selected?.path === file.path
            return (
              <li key={file.path}>
                <button
                  type="button"
                  className={`lgtm-fileitem${active ? ' is-active' : ''}`}
                  onClick={() => r.select(file)}
                >
                  <span className={`lgtm-status lgtm-status-${file.status}`}>{file.status}</span>
                  <span className="lgtm-path">{file.path}</span>
                </button>
              </li>
            )
          })}
          {!r.files.length && !r.loading ? (
            <li className="lgtm-empty">No changed files</li>
          ) : null}
        </ul>
      </aside>
      <main className="lgtm-diff">
        <header className="lgtm-diff-header">
          <span>{r.selected?.path ?? 'Select a file'}</span>
          {r.loading ? <span className="lgtm-loading">…</span> : null}
        </header>
        {r.error ? <div className="lgtm-error">{r.error}</div> : null}
        <pre className="lgtm-diff-body">{r.diffText}</pre>
      </main>
    </div>
  )
}
