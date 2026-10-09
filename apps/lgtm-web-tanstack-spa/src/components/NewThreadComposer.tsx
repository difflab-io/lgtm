// Inline composer that opens below a diff line when the reviewer clicks a +
// icon. Submits a new thread anchored at that (file, line).

import { useState } from 'react'

export function NewThreadComposer(props: {
  file: string
  line: number
  onCreate: (body: string) => Promise<void>
  onCancel: () => void
}) {
  const [body, setBody] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit() {
    const trimmed = body.trim()
    if (!trimmed || busy) return
    setBusy(true)
    try {
      await props.onCreate(trimmed)
      setBody('')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="lgtm-thread lgtm-thread-draft">
      <header className="lgtm-thread-header">
        <span>
          New thread · {props.file}
          {props.line > 0 ? `:${props.line}` : ''}
        </span>
        <button type="button" className="lgtm-btn-ghost" onClick={props.onCancel}>
          Cancel
        </button>
      </header>
      <form
        className="lgtm-reply"
        onSubmit={(e) => {
          e.preventDefault()
          void submit()
        }}
      >
        <textarea
          className="lgtm-reply-input"
          placeholder="Leave a comment…"
          value={body}
          onChange={(e) => setBody(e.target.value)}
          autoFocus
          rows={3}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
              e.preventDefault()
              void submit()
            }
            if (e.key === 'Escape') {
              e.preventDefault()
              props.onCancel()
            }
          }}
        />
        <button type="submit" className="lgtm-btn" disabled={busy || !body.trim()}>
          Comment
        </button>
      </form>
    </div>
  )
}
