// Classify unified-diff lines so the view can color them.
//
// Mirror of apps/lgtm-desktop-tauri/placeholder/review.js (classifyDiff).
// Keep in sync when either side changes.

export type DiffLineKind = 'added' | 'removed' | 'context' | 'hunk' | 'header'

export interface DiffLine {
  kind: DiffLineKind
  text: string
}

export function parseDiff(diffText: string): DiffLine[] {
  if (!diffText) return []
  const out: DiffLine[] = []
  const lines = diffText.split('\n')
  // The last split may be empty if the diff ends with a newline; drop it.
  if (lines.length > 0 && lines[lines.length - 1] === '') lines.pop()

  for (const text of lines) {
    out.push({ kind: classify(text), text })
  }
  return out
}

function classify(line: string): DiffLineKind {
  if (
    line.startsWith('diff --git') ||
    line.startsWith('index ') ||
    line.startsWith('--- ') ||
    line.startsWith('+++ ')
  ) {
    return 'header'
  }
  if (line.startsWith('@@')) return 'hunk'
  if (line.startsWith('+')) return 'added'
  if (line.startsWith('-')) return 'removed'
  return 'context'
}
