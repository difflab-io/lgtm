/** A fixed range in a backend-owned review revision. */
export const line = (snapshotId: string, path = '') => ({
  kind: 'line',
  snapshotId,
  path,
  side: 'new',
  start: '2',
  end: '2',
})
