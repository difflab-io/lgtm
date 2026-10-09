// Two-pane review shell. Left: file list. Right: raw diff text.
// Visual target for GPUI/Tauri parity.

import { useEffect, useMemo, useState } from 'react'

import type { ReviewConfig } from '../lib/review-state'
import { useReview } from '../lib/review-state'
import { parseDiff, type DiffLine } from '../lib/diff-lines'
import { highlightLine, langForPath } from '../lib/highlight'

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
        <DiffBody diffText={r.diffText} filePath={r.selected?.path ?? ''} />
      </main>
    </div>
  )
}

/**
 * Diff viewer with Shiki-powered syntax highlighting for context + added +
 * removed lines. Falls back to plain text while Shiki loads (and for any
 * line whose language isn't registered). Hunk/header lines are not
 * highlighted — they carry colour from the diff line class.
 */
function DiffBody(props: { diffText: string; filePath: string }) {
  const lines = useMemo(() => parseDiff(props.diffText), [props.diffText])
  const lang = useMemo(() => langForPath(props.filePath), [props.filePath])
  const [highlighted, setHighlighted] = useState<ReadonlyArray<string | null>>(() =>
    lines.map(() => null),
  )

  useEffect(() => {
    let cancelled = false
    setHighlighted(lines.map(() => null))
    if (!lang) return
    Promise.all(lines.map((line) => highlightLineFor(line, lang)))
      .then((rendered) => {
        if (!cancelled) setHighlighted(rendered)
      })
      .catch(() => {
        /* ignore highlighter failures — plain text remains */
      })
    return () => {
      cancelled = true
    }
  }, [lines, lang])

  return (
    <pre className="lgtm-diff-body">
      {lines.map((line, i) => {
        const inner = highlighted[i]
        const marker = line.kind === 'added' || line.kind === 'removed' ? line.text[0] : null
        return (
          <span key={i} className={`lgtm-line lgtm-line-${line.kind}`}>
            {marker ? <span className="lgtm-marker">{marker}</span> : null}
            {inner != null ? (
              <span dangerouslySetInnerHTML={{ __html: inner }} />
            ) : (
              <span>{marker ? line.text.slice(1) : line.text}</span>
            )}
            {'\n'}
          </span>
        )
      })}
    </pre>
  )
}

function highlightLineFor(line: DiffLine, lang: string): Promise<string | null> {
  if (line.kind === 'header' || line.kind === 'hunk') return Promise.resolve(null)
  const body =
    line.kind === 'added' || line.kind === 'removed' ? line.text.slice(1) : line.text
  return highlightLine(body, lang)
}
