// Composite review-session hook: owns changed-files, selected file, diff,
// threads, and the exposed verbs (reload, select, createThread, appendComment,
// resolveThread). Keeping all state in one hook makes it trivial to mirror
// the behavior in GPUI and keeps the component tree thin.

import { useCallback, useEffect, useMemo, useState } from 'react'

import {
  LgtmClient,
  type ChangedFile,
  type Thread,
  type User,
} from './lgtm-client'
import { ensureUser, loadIdentity, saveIdentity, type Identity } from './identity'

export interface ReviewConfig {
  baseUrl: string
  repoPath: string
  baseRef: string
}

export interface ReviewState {
  files: ChangedFile[]
  selected: ChangedFile | null
  diffText: string
  threads: Thread[]
  identity: Identity
  user: User | null
  loading: boolean
  error: string | null
  select: (file: ChangedFile) => void
  reload: () => void
  setIdentity: (next: Identity) => void
  createThread: (file: string, line: number, body: string) => Promise<void>
  appendComment: (threadId: number, body: string) => Promise<void>
  resolveThread: (threadId: number) => Promise<void>
}

export function useReview(config: ReviewConfig): ReviewState {
  const client = useMemo(() => new LgtmClient(config.baseUrl), [config.baseUrl])
  const [files, setFiles] = useState<ChangedFile[]>([])
  const [selected, setSelected] = useState<ChangedFile | null>(null)
  const [diffText, setDiffText] = useState('')
  const [threads, setThreads] = useState<Thread[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [identity, setIdentityState] = useState<Identity>(() => loadIdentity())
  const [user, setUser] = useState<User | null>(null)

  // Resolve identity → User on load and whenever identity changes.
  useEffect(() => {
    let cancelled = false
    ensureUser(client, identity)
      .then((u) => {
        if (!cancelled) setUser(u)
      })
      .catch((e) => {
        if (!cancelled) setError((e as Error).message)
      })
    return () => {
      cancelled = true
    }
  }, [client, identity])

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

  // Load diff + threads whenever the selected file (or base) changes.
  useEffect(() => {
    if (!selected) {
      setDiffText('')
      setThreads([])
      return
    }
    let cancelled = false
    setLoading(true)
    Promise.all([
      client.getFileDiff({
        repo_path: config.repoPath,
        base_ref: config.baseRef,
        path: selected.path,
      }),
      client.listThreads({
        repo_path: config.repoPath,
        base_ref: config.baseRef,
        file_path: selected.path,
      }),
    ])
      .then(([diffRes, threadsRes]) => {
        if (cancelled) return
        setDiffText(diffRes.diff_text)
        setThreads(threadsRes.threads)
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

  const setIdentity = useCallback((next: Identity) => {
    saveIdentity(next)
    setIdentityState(next)
  }, [])

  const createThread = useCallback(
    async (file: string, line: number, body: string) => {
      if (!user) throw new Error('identity not yet resolved')
      const res = await client.createThread({
        repo_path: config.repoPath,
        base_ref: config.baseRef,
        file_path: file,
        line,
        author_id: user.id,
        body,
      })
      setThreads((prev) => [...prev, res.thread])
    },
    [client, config.repoPath, config.baseRef, user],
  )

  const appendComment = useCallback(
    async (threadId: number, body: string) => {
      if (!user) throw new Error('identity not yet resolved')
      const res = await client.appendComment({
        thread_id: threadId,
        author_id: user.id,
        body,
      })
      setThreads((prev) =>
        prev.map((t) =>
          t.id === threadId ? { ...t, comments: [...t.comments, res.comment] } : t,
        ),
      )
    },
    [client, user],
  )

  const resolveThread = useCallback(
    async (threadId: number) => {
      const res = await client.resolveThread({ thread_id: threadId })
      setThreads((prev) => prev.map((t) => (t.id === threadId ? res.thread : t)))
    },
    [client],
  )

  return {
    files,
    selected,
    diffText,
    threads,
    identity,
    user,
    loading,
    error,
    select: setSelected,
    reload: loadFiles,
    setIdentity,
    createThread,
    appendComment,
    resolveThread,
  }
}
