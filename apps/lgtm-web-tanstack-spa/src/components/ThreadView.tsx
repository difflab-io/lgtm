// Rendered body of one comment thread: existing comments, reply composer,
// resolve button. Used both inline (below a diff line) and in the sidebar.

import { useState } from 'react'

import type { Comment, Thread } from '../lib/lgtm-client'

export function ThreadView(props: {
  thread: Thread
  onReply: (body: string) => Promise<void>
  onResolve: () => Promise<void>
}) {
  const { thread, onReply, onResolve } = props
  const [reply, setReply] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit() {
    const body = reply.trim()
    if (!body || busy) return
    setBusy(true)
    try {
      await onReply(body)
      setReply('')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className={`lgtm-thread${thread.resolved ? ' is-resolved' : ''}`}>
      <header className="lgtm-thread-header">
        <span>
          Thread #{thread.id} · {thread.file_path}
          {thread.line > 0 ? `:${thread.line}` : ''} ·{' '}
          <span className="lgtm-thread-state">
            {thread.resolved ? 'resolved' : 'open'}
          </span>
        </span>
        {!thread.resolved ? (
          <button
            type="button"
            className="lgtm-btn-ghost"
            onClick={() => void onResolve()}
          >
            Resolve
          </button>
        ) : null}
      </header>
      <ol className="lgtm-comments">
        {thread.comments.map((c) => (
          <li key={c.id}>
            <CommentView comment={c} />
          </li>
        ))}
      </ol>
      {!thread.resolved ? (
        <form
          className="lgtm-reply"
          onSubmit={(e) => {
            e.preventDefault()
            void submit()
          }}
        >
          <textarea
            className="lgtm-reply-input"
            placeholder="Reply…"
            value={reply}
            onChange={(e) => setReply(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
                e.preventDefault()
                void submit()
              }
            }}
            rows={2}
          />
          <button type="submit" className="lgtm-btn" disabled={busy || !reply.trim()}>
            Reply
          </button>
        </form>
      ) : null}
    </div>
  )
}

function CommentView({ comment }: { comment: Comment }) {
  const kind = comment.author.kind === 'AUTHOR_KIND_AGENT' ? 'agent' : 'user'
  return (
    <div className="lgtm-comment">
      <header className="lgtm-comment-author">
        <span className={`lgtm-author-badge lgtm-author-${kind}`}>{kind}</span>
        <span className="lgtm-author-name">{comment.author.name}</span>
        <time className="lgtm-comment-time">{formatTime(comment.created_at)}</time>
      </header>
      <div className="lgtm-comment-body">{comment.body}</div>
    </div>
  )
}

function formatTime(epochSec: number): string {
  const d = new Date(epochSec * 1000)
  const now = Date.now()
  const diffS = Math.max(0, Math.floor((now - d.getTime()) / 1000))
  if (diffS < 60) return 'just now'
  if (diffS < 3600) return `${Math.floor(diffS / 60)}m ago`
  if (diffS < 86_400) return `${Math.floor(diffS / 3600)}h ago`
  return d.toISOString().slice(0, 10)
}
