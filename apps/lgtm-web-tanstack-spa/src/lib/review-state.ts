// Minimal review-session hook used by the TanStack SPA shell. One hook keeps
// the shared code tiny and readable; parity targets with the GPUI/Tauri apps
// should modify this surface, not the component tree directly.

import { useCallback, useEffect, useMemo, useState } from 'react'

import { LgtmClient, type ChangedFile } from './lgtm-client'

export interface ReviewConfig {
  baseUrl: string
  repoPath: string
  baseRef: string
}

export interface ReviewState {
  files: ChangedFile[]
  selected: ChangedFile | null
  diffText: string
  loading: boolean
  error: string | null
  select: (file: ChangedFile) => void
  reload: () => void
}

export function useReview(config: ReviewConfig): ReviewState {
  const client = useMemo(() => new LgtmClient(config.baseUrl), [config.baseUrl])
  const [files, setFiles] = useState<ChangedFile[]>([])
  const [selected, setSelected] = useState<ChangedFile | null>(null)
  const [diffText, setDiffText] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadFiles = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const res = await client.getChangedFiles({
        repo_path: config.repoPath,
        base_ref: config.baseRef,
      })
      setFiles(res.files)
      setSelected((prev) => prev ?? res.files[0] ?? null)
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setLoading(false)
    }
  }, [client, config.repoPath, config.baseRef])

  useEffect(() => {
    void loadFiles()
  }, [loadFiles])

  useEffect(() => {
    if (!selected) {
      setDiffText('')
      return
    }
    let cancelled = false
    setLoading(true)
    client
      .getFileDiff({
        repo_path: config.repoPath,
        base_ref: config.baseRef,
        path: selected.path,
      })
      .then((res) => {
        if (!cancelled) setDiffText(res.diff_text)
      })
      .catch((e) => {
        if (!cancelled) setError((e as Error).message)
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [client, selected, config.repoPath, config.baseRef])

  return {
    files,
    selected,
    diffText,
    loading,
    error,
    select: setSelected,
    reload: loadFiles,
  }
}
