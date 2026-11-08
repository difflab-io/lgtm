// TODO(keybinds): Remove temporary keybindings in favor of @cloudvoyant/helical-ui HotkeyService / WhichKey integration before merge.
import { goToNextChunk, goToPreviousChunk } from '@codemirror/merge'
import { keymap, type EditorView, type KeyBinding } from '@codemirror/view'

/**
 * Move the active diff editor selection to the next changed hunk.
 */
export function goToNextHunk(view: EditorView | undefined): boolean {
  if (!view) return false
  return goToNextChunk(view)
}

/**
 * Move the active diff editor selection to the previous changed hunk.
 */
export function goToPrevHunk(view: EditorView | undefined): boolean {
  if (!view) return false
  return goToPreviousChunk(view)
}

/**
 * Isolated CodeMirror 6 keybindings for hunk navigation inside `DiffEditor`.
 */
export const DIFF_EDITOR_KEYBINDINGS: readonly KeyBinding[] = [
  {
    key: 'Alt-PageDown',
    run: (view) => goToNextChunk(view),
  },
  {
    key: 'Alt-PageUp',
    run: (view) => goToPreviousChunk(view),
  },
]

/**
 * CodeMirror 6 keymap extension for navigating between diff hunks.
 */
export const diffHunkKeymap = keymap.of(DIFF_EDITOR_KEYBINDINGS)
