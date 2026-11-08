// TODO(keybinds): Remove temporary keybindings and window dispatcher in favor of @cloudvoyant/helical-ui HotkeyService / WhichKey integration before merge.

/** Action identifier for diff and review keyboard shortcuts. */
export type WebKeybindingActionId =
  | 'diff.nextFile'
  | 'diff.prevFile'
  | 'diff.nextHunk'
  | 'diff.prevHunk'
  | 'diff.toggleViewMode'
  | 'diff.toggleFullFile'
  | 'review.toggleMode'

/** Metadata entry describing a diff or review keyboard shortcut. */
export interface WebKeybindingDefinition {
  /** Stable action identifier. */
  id: WebKeybindingActionId
  /** Human-readable label. */
  label: string
  /** Primary shortcut sequence. */
  shortcut: string
  /** Secondary chord or key combination. */
  chord?: string
  /** Shortcut category (`diff` or `review`). */
  category: 'diff' | 'review'
}

/** Temporary registry of diff and review keyboard shortcuts isolated for Phase 5 migration. */
export const WEB_KEYBINDINGS: readonly WebKeybindingDefinition[] = [
  {
    id: 'diff.nextFile',
    label: 'Next changed file',
    shortcut: 'Alt+ArrowDown',
    chord: ']f',
    category: 'diff',
  },
  {
    id: 'diff.prevFile',
    label: 'Previous changed file',
    shortcut: 'Alt+ArrowUp',
    chord: '[f',
    category: 'diff',
  },
  {
    id: 'diff.nextHunk',
    label: 'Next diff hunk',
    shortcut: 'Alt+PageDown',
    chord: ']c',
    category: 'diff',
  },
  {
    id: 'diff.prevHunk',
    label: 'Previous diff hunk',
    shortcut: 'Alt+PageUp',
    chord: '[c',
    category: 'diff',
  },
  {
    id: 'diff.toggleViewMode',
    label: 'Toggle split / unified diff view',
    shortcut: 'Alt+m',
    category: 'diff',
  },
  {
    id: 'diff.toggleFullFile',
    label: 'Toggle hunks / full file diff view',
    shortcut: 'Alt+f',
    category: 'diff',
  },
  {
    id: 'review.toggleMode',
    label: 'Toggle Local / Branch review mode',
    shortcut: 'Alt+r',
    category: 'review',
  },
]

/** Callback map invoked by `createKeybindingDispatcher`. */
export type WebKeybindingHandlers = Partial<Record<WebKeybindingActionId, () => void>>

function isEditableElement(target: EventTarget | null): boolean {
  if (typeof HTMLElement === 'undefined' || !(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  const tag = target.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
}

/** Creates a window `keydown` listener that dispatches diff and review actions from `WEB_KEYBINDINGS`. */
export function createKeybindingDispatcher(
  handlers: WebKeybindingHandlers,
): (event: KeyboardEvent) => void {
  let pendingBracket: '[' | ']' | null = null
  let bracketTimer: ReturnType<typeof setTimeout> | null = null

  const clearPendingBracket = () => {
    pendingBracket = null
    if (bracketTimer !== null) {
      clearTimeout(bracketTimer)
      bracketTimer = null
    }
  }

  return (event: KeyboardEvent) => {
    if (event.altKey && !event.metaKey && !event.ctrlKey) {
      clearPendingBracket()
      if (event.key === 'ArrowDown' || event.code === 'ArrowDown') {
        event.preventDefault()
        handlers['diff.nextFile']?.()
        return
      }
      if (event.key === 'ArrowUp' || event.code === 'ArrowUp') {
        event.preventDefault()
        handlers['diff.prevFile']?.()
        return
      }
      if (event.key === 'PageDown' || event.code === 'PageDown') {
        event.preventDefault()
        handlers['diff.nextHunk']?.()
        return
      }
      if (event.key === 'PageUp' || event.code === 'PageUp') {
        event.preventDefault()
        handlers['diff.prevHunk']?.()
        return
      }
      if (event.key === 'm' || event.key === 'M' || event.code === 'KeyM') {
        event.preventDefault()
        handlers['diff.toggleViewMode']?.()
        return
      }
      if (event.key === 'f' || event.key === 'F' || event.code === 'KeyF') {
        event.preventDefault()
        handlers['diff.toggleFullFile']?.()
        return
      }
      if (event.key === 'r' || event.key === 'R' || event.code === 'KeyR') {
        event.preventDefault()
        handlers['review.toggleMode']?.()
        return
      }
    }

    if (event.metaKey || event.ctrlKey || event.altKey || isEditableElement(event.target)) {
      clearPendingBracket()
      return
    }

    if (pendingBracket !== null) {
      const prefix = pendingBracket
      clearPendingBracket()
      if (event.key === 'f') {
        event.preventDefault()
        if (prefix === ']') {
          handlers['diff.nextFile']?.()
        } else {
          handlers['diff.prevFile']?.()
        }
        return
      }
      if (event.key === 'c') {
        event.preventDefault()
        if (prefix === ']') {
          handlers['diff.nextHunk']?.()
        } else {
          handlers['diff.prevHunk']?.()
        }
        return
      }
    }

    if (event.key === '[' || event.key === ']') {
      pendingBracket = event.key
      bracketTimer = setTimeout(() => {
        pendingBracket = null
        bracketTimer = null
      }, 600)
    }
  }
}
