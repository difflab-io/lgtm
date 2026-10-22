import { hash } from '../common/test-utils.js'

/** A line selection from exact local bytes, including its surrounding evidence. */
export const line = (bytes: string, path = '') => ({
  kind: 'line',
  path,
  side: 'new',
  start: '2',
  end: '2',
  source: { localFile: { contentHash: hash(bytes) } },
  evidence: { selectedText: 'selected', contextBefore: 'before', contextAfter: 'after' },
})
