// Three-pane review shell. Left: file list. Middle: diff + inline comment
// threads. Right: threads sidebar + identity badge. Visual target for GPUI
// parity.

import { Fragment, useEffect, useMemo, useState } from 'react'

import type { Thread } from '../lib/lgtm-client'
import type { ReviewConfig } from '../lib/review-state'
import { useReview } from '../lib/review-state'
import { parseDiff, type DiffLine } from '../lib/diff-lines'
import { highlightLine, langForPath } from '../lib/highlight'
import { ThreadView } from './ThreadView'
import { NewThreadComposer } from './NewThreadComposer'

export function ReviewLayout(props: { config: ReviewConfig }) {
  const r = useReview(props.config)
  const [composingLine, setComposingLine] = useState<number | null>(null)
  const [onlyUnresolved, setOnlyUnresolved] = useState(false)

  // Threads indexed by line for the current file. Line 0 = file-level.
  const threadsByLine = useMemo(() => groupThreadsByLine(r.threads), [r.threads])
  const visibleThreads = useMemo(
    () => (onlyUnresolved ? r.threads.filter((t) => !t.resolved) : r.threads),
    [r.threads, onlyUnresolved],
  )

  // Reset the draft composer when the file changes.
  useEffect(() => {
    setComposingLine(null)
  }, [r.selected?.path])

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
            const openThreadCount = r.threads.filter(
              (t) => t.file_path === file.path && !t.resolved,
            ).length
            return (
              <li key={file.path}>
                <button
                  type="button"
                  className={`lgtm-fileitem${active ? ' is-active' : ''}`}
                  onClick={() => r.select(file)}
                >
                  <span className={`lgtm-status lgtm-status-${file.status}`}>
                    {file.status}
                  </span>
                  <span className="lgtm-path">{file.path}</span>
                  {openThreadCount > 0 ? (
                    <span className="lgtm-badge">{openThreadCount}</span>
                  ) : null}
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
          {r.selected ? (
            <button
              type="button"
              className="lgtm-btn-ghost"
              onClick={() => setComposingLine(0)}
            >
              New file-level thread
            </button>
          ) : null}
        </header>
        {r.error ? <div className="lgtm-error">{r.error}</div> : null}
        <DiffBody
          diffText={r.diffText}
          filePath={r.selected?.path ?? ''}
          threadsByLine={threadsByLine}
          composingLine={composingLine}
          onStartCompose={(line) => setComposingLine(line)}
          onCancelCompose={() => setComposingLine(null)}
          onCreateThread={async (line, body) => {
            if (!r.selected) return
            await r.createThread(r.selected.path, line, body)
            setComposingLine(null)
          }}
          onReply={(tid, body) => r.appendComment(tid, body)}
          onResolve={(tid) => r.resolveThread(tid)}
        />
      </main>

      <aside className="lgtm-threads-sidebar">
        <header className="lgtm-sidebar-header">
          <span>Threads · {visibleThreads.length}</span>
          <label className="lgtm-check">
            <input
              type="checkbox"
              checked={onlyUnresolved}
              onChange={(e) => setOnlyUnresolved(e.target.checked)}
            />{' '}
            open only
          </label>
        </header>
        <IdentityBadge
          identity={r.identity}
          userName={r.user?.name}
          onChange={r.setIdentity}
        />
        <div className="lgtm-threads-list">
          {visibleThreads.length === 0 ? (
            <div className="lgtm-empty">No threads for this file</div>
          ) : null}
          {visibleThreads.map((t) => (
            <ThreadView
              key={t.id}
              thread={t}
              onReply={(body) => r.appendComment(t.id, body)}
              onResolve={() => r.resolveThread(t.id)}
            />
          ))}
        </div>
      </aside>
    </div>
  )
}

function groupThreadsByLine(threads: Thread[]): Map<number, Thread[]> {
  const out = new Map<number, Thread[]>()
  for (const t of threads) {
    const list = out.get(t.line) ?? []
    list.push(t)
    out.set(t.line, list)
  }
  return out
}

// -------- Diff body --------

interface DiffBodyProps {
  diffText: string
  filePath: string
  threadsByLine: Map<number, Thread[]>
  composingLine: number | null
  onStartCompose: (line: number) => void
  onCancelCompose: () => void
  onCreateThread: (line: number, body: string) => Promise<void>
  onReply: (threadId: number, body: string) => Promise<void>
  onResolve: (threadId: number) => Promise<void>
}

function DiffBody(props: DiffBodyProps) {
  const diffLines = useMemo(() => parseDiff(props.diffText), [props.diffText])
  const lang = useMemo(() => langForPath(props.filePath), [props.filePath])
  const [highlighted, setHighlighted] = useState<ReadonlyArray<string | null>>(() =>
    diffLines.map(() => null),
  )

  useEffect(() => {
    let cancelled = false
    setHighlighted(diffLines.map(() => null))
    if (!lang) return
    Promise.all(diffLines.map((line) => highlightLineFor(line, lang)))
      .then((rendered) => {
        if (!cancelled) setHighlighted(rendered)
      })
      .catch(() => {
        /* ignore highlighter failures — plain text remains */
      })
    return () => {
      cancelled = true
    }
  }, [diffLines, lang])

  // Running counter for new-file line numbers. +N starts at the hunk header,
  // context (" ") increments it, "-" lines don't move it, "+" lines consume
  // the current position and increment.
  const lineNumbers = useMemo(() => computeNewLineNumbers(diffLines), [diffLines])

  return (
    <div className="lgtm-diff-body">
      {diffLines.map((line, i) => {
        const inner = highlighted[i]
        const marker = line.kind === 'added' || line.kind === 'removed' ? line.text[0] : null
        const n = lineNumbers[i]
        const anchorable = line.kind === 'added' || line.kind === 'context'
        const threads = n > 0 ? (props.threadsByLine.get(n) ?? []) : []
        const isComposing = props.composingLine === n && n > 0

        return (
          <Fragment key={i}>
            <div className={`lgtm-line lgtm-line-${line.kind}`}>
              <span className="lgtm-ln">{n > 0 ? n : ''}</span>
              <span className="lgtm-marker">{marker ?? ' '}</span>
              {anchorable && n > 0 ? (
                <button
                  type="button"
                  className="lgtm-addthread"
                  title={`Comment on line ${n}`}
                  onClick={() => props.onStartCompose(n)}
                >
                  +
                </button>
              ) : (
                <span className="lgtm-addthread lgtm-addthread-spacer" />
              )}
              <span className="lgtm-code">
                {inner != null ? (
                  <span dangerouslySetInnerHTML={{ __html: inner }} />
                ) : (
                  <span>{marker ? line.text.slice(1) : line.text}</span>
                )}
              </span>
            </div>
            {threads.map((t) => (
              <div key={`t-${t.id}`} className="lgtm-inline-thread">
                <ThreadView
                  thread={t}
                  onReply={(body) => props.onReply(t.id, body)}
                  onResolve={() => props.onResolve(t.id)}
                />
              </div>
            ))}
            {isComposing ? (
              <div className="lgtm-inline-thread">
                <NewThreadComposer
                  file={props.filePath}
                  line={n}
                  onCreate={(body) => props.onCreateThread(n, body)}
                  onCancel={props.onCancelCompose}
                />
              </div>
            ) : null}
          </Fragment>
        )
      })}
    </div>
  )
}

function highlightLineFor(line: DiffLine, lang: string): Promise<string | null> {
  if (line.kind === 'header' || line.kind === 'hunk') return Promise.resolve(null)
  const body =
    line.kind === 'added' || line.kind === 'removed' ? line.text.slice(1) : line.text
  return highlightLine(body, lang)
}

function computeNewLineNumbers(lines: ReadonlyArray<DiffLine>): number[] {
  const out = new Array<number>(lines.length).fill(0)
  let cursor = 0
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i]
    if (l.kind === 'hunk') {
      const m = /\+(\d+)/.exec(l.text)
      cursor = m ? Number(m[1]) : 0
      continue
    }
    if (l.kind === 'header') continue
    if (l.kind === 'removed') {
      // No new-file line number.
      continue
    }
    // added or context
    if (cursor > 0) {
      out[i] = cursor
      cursor++
    }
  }
  return out
}

// -------- Identity --------

function IdentityBadge(props: {
  identity: { name: string; kind: 'AUTHOR_KIND_USER' | 'AUTHOR_KIND_AGENT' | 'AUTHOR_KIND_UNSPECIFIED' }
  userName: string | undefined
  onChange: (next: typeof props.identity) => void
}) {
  const [editing, setEditing] = useState(false)
  const [name, setName] = useState(props.identity.name)
  const [kind, setKind] = useState(props.identity.kind)
  const label = props.identity.kind === 'AUTHOR_KIND_AGENT' ? 'agent' : 'user'

  if (!editing) {
    return (
      <div className="lgtm-identity">
        <span>
          Commenting as <span className={`lgtm-author-badge lgtm-author-${label}`}>{label}</span>{' '}
          <strong>{props.userName ?? props.identity.name}</strong>
        </span>
        <button
          type="button"
          className="lgtm-btn-ghost"
          onClick={() => {
            setName(props.identity.name)
            setKind(props.identity.kind)
            setEditing(true)
          }}
        >
          Change
        </button>
      </div>
    )
  }

  return (
    <form
      className="lgtm-identity lgtm-identity-edit"
      onSubmit={(e) => {
        e.preventDefault()
        props.onChange({ name: name.trim() || 'you', kind })
        setEditing(false)
      }}
    >
      <input
        type="text"
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="lgtm-input"
        placeholder="your name"
      />
      <select
        value={kind}
        onChange={(e) => setKind(e.target.value as typeof kind)}
        className="lgtm-input"
      >
        <option value="AUTHOR_KIND_USER">user</option>
        <option value="AUTHOR_KIND_AGENT">agent</option>
      </select>
      <button type="submit" className="lgtm-btn">
        Save
      </button>
    </form>
  )
}
