import type { TerminalSurfaceHandle } from 'lgtm-ui'
import type {
  TerminalAgentStatus,
  TerminalExitEvent,
  TerminalSession,
  TerminalStreamEvent,
  WorkspaceClient,
} from '../clients/workspaceClient'

/** Notification payload emitted when a background worktree/agent becomes `done` or `blocked`. */
export interface TerminalWorktreeNotification {
  sessionId: string
  workspaceRoot: string
  status: 'done' | 'blocked'
  title: string
  message: string
}

/** Callbacks linking `TerminalService` with peer workspace and notification hosts without circular imports. */
export interface TerminalServicePeers {
  getRoot: () => string
  isRightDockOpen: () => boolean
  openRightDock: () => void
  selectWorkspace: (path: string) => Promise<void>
  onNotification?: (notification: TerminalWorktreeNotification) => void
}

const STATUS_PRIORITY: Record<TerminalAgentStatus, number> = {
  blocked: 3,
  working: 2,
  done: 1,
  idle: 0,
}

/**
 * Frontend domain service managing `herdr-core` terminal tabs, active-surface attachment,
 * dynamic `AgentsPane` entries, Herdr-style worktree status dots, and background notifications.
 */
export class TerminalService {
  private readonly client: WorkspaceClient
  private readonly peers: TerminalServicePeers
  private active = true
  private attachedSessionId: string | null = null
  private activeSurfaceHandle: TerminalSurfaceHandle | null = null
  private lastCols = 80
  private lastRows = 24
  private readonly notifiedSeqBySession = new Map<string, number>()

  /** All known live `herdr-core` sessions keyed by session ID. */
  sessions = $state<TerminalSession[]>([])
  /** Active terminal tab ID remembered per workspace root. */
  activeTabByWorkspace = $state<Record<string, string>>({})
  /** Last terminal IPC error surfaced in the UI, if any. */
  error = $state('')

  /** Constructs a `TerminalService` wired to `client` and peer workspace callbacks. */
  constructor(client: WorkspaceClient, peers: TerminalServicePeers) {
    this.client = client
    this.peers = peers
  }

  // Public --------------------------------------------------------------------

  /** Ordered live terminal tabs belonging to the currently active workspace. */
  get activeWorkspaceTabs(): TerminalSession[] {
    const root = this.peers.getRoot()
    if (!root) return []
    return this.sessions.filter((s) => s.alive && s.workspaceRoot === root)
  }

  /** Active terminal session ID for the currently focused workspace, if any. */
  get activeSessionId(): string | null {
    const root = this.peers.getRoot()
    const tabs = this.activeWorkspaceTabs
    if (!root || tabs.length === 0) return null
    const remembered = this.activeTabByWorkspace[root]
    if (remembered && tabs.some((t) => t.id === remembered)) {
      return remembered
    }
    return tabs[0].id
  }

  /** Active `TerminalSession` object for the currently focused workspace, if any. */
  get activeSession(): TerminalSession | null {
    const id = this.activeSessionId
    if (!id) return null
    return this.activeWorkspaceTabs.find((t) => t.id === id) ?? null
  }

  /** All live sessions with a dynamically detected coding agent, sorted by `(status_priority DESC, state_change_seq DESC)`. */
  get detectedAgents(): TerminalSession[] {
    return this.sessions
      .filter((s) => s.alive && Boolean(s.agent))
      .toSorted((a, b) => {
        const prioDiff = STATUS_PRIORITY[b.status] - STATUS_PRIORITY[a.status]
        if (prioDiff !== 0) return prioDiff
        return b.stateChangeSeq - a.stateChangeSeq
      })
  }

  /** Returns the aggregated Herdr-style status dot for `workspaceRoot` (`blocked > working > done > idle`), or `null` if no live terminals exist. */
  getWorkspaceStatus(workspaceRoot: string): TerminalAgentStatus | null {
    let best: TerminalAgentStatus | null = null
    for (const session of this.sessions) {
      if (!session.alive || session.workspaceRoot !== workspaceRoot) continue
      if (!best || STATUS_PRIORITY[session.status] > STATUS_PRIORITY[best]) {
        best = session.status
      }
    }
    return best
  }

  /** Fetches all live `herdr-core` sessions across workspaces to hydrate `RightDock`, `AgentsPane`, and `ProjectsPane` dots. */
  async syncAllSessions(): Promise<void> {
    if (!this.client.terminalList || !this.active) return
    try {
      const list = await this.client.terminalList(null)
      if (!this.active) return
      this.sessions = list.filter((s) => s.alive)
      this.error = ''
    } catch (err) {
      if (this.active) {
        this.error = err instanceof Error ? err.message : String(err)
      }
    }
  }

  /** Ensures the active workspace has at least one terminal tab when `RightDock` is open. */
  async ensureActiveWorkspaceTerminal(workspaceRoot = this.peers.getRoot()): Promise<void> {
    if (!workspaceRoot || !this.active) return
    await this.syncAllSessions()
    if (!this.active) return
    const existing = this.sessions.filter((s) => s.alive && s.workspaceRoot === workspaceRoot)
    if (existing.length > 0) {
      if (!this.activeTabByWorkspace[workspaceRoot]) {
        this.activeTabByWorkspace = {
          ...this.activeTabByWorkspace,
          [workspaceRoot]: existing[0].id,
        }
      }
      return
    }
    await this.createTab(workspaceRoot)
  }

  /** Spawns a new persistent `herdr-core` terminal tab in `workspaceRoot` and selects it. */
  async createTab(workspaceRoot = this.peers.getRoot()): Promise<TerminalSession | null> {
    if (!this.client.terminalCreate || !workspaceRoot) return null
    try {
      const created = await this.client.terminalCreate(workspaceRoot, this.lastCols, this.lastRows)
      if (!this.active) return null
      this.upsertSession(created)
      this.activeTabByWorkspace = {
        ...this.activeTabByWorkspace,
        [workspaceRoot]: created.id,
      }
      this.error = ''
      return created
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err)
      return null
    }
  }

  /** Selects `sessionId` as the active terminal tab for its workspace and clears any unseen `done` dot. */
  async selectTab(sessionId: string): Promise<void> {
    const target = this.sessions.find((s) => s.id === sessionId)
    if (!target) return
    this.activeTabByWorkspace = {
      ...this.activeTabByWorkspace,
      [target.workspaceRoot]: target.id,
    }
    await this.markSeen(sessionId)
  }

  /** Cycles to the next terminal tab in the active workspace (`count` steps). */
  async selectNextTab(count = 1): Promise<void> {
    const tabs = this.activeWorkspaceTabs
    if (tabs.length <= 1) return
    const currentIdx = Math.max(
      0,
      tabs.findIndex((t) => t.id === this.activeSessionId),
    )
    const nextIdx = (currentIdx + count) % tabs.length
    await this.selectTab(tabs[nextIdx].id)
  }

  /** Cycles to the previous terminal tab in the active workspace (`count` steps). */
  async selectPrevTab(count = 1): Promise<void> {
    const tabs = this.activeWorkspaceTabs
    if (tabs.length <= 1) return
    const currentIdx = Math.max(
      0,
      tabs.findIndex((t) => t.id === this.activeSessionId),
    )
    const prevIdx = (currentIdx - (count % tabs.length) + tabs.length) % tabs.length
    await this.selectTab(tabs[prevIdx].id)
  }

  /** Closes and terminates `sessionId` in `herdr-core`. */
  async closeTab(sessionId: string): Promise<void> {
    if (!this.client.terminalClose) return
    try {
      await this.client.terminalClose(sessionId)
    } catch {
      // Ignore already-closed terminal errors.
    }
    this.removeSession(sessionId)
  }

  /** Closes the active terminal tab in the current workspace. */
  async closeActiveTab(): Promise<void> {
    const id = this.activeSessionId
    if (id) {
      await this.closeTab(id)
    }
  }

  /** Attaches the single mounted `TerminalSurface` handle to `sessionId`, detaching any prior stream. */
  async attachSurface(
    sessionId: string,
    cols: number,
    rows: number,
    handle: TerminalSurfaceHandle,
  ): Promise<void> {
    if (!this.client.terminalAttach || !this.active) return
    this.lastCols = Math.max(20, cols)
    this.lastRows = Math.max(4, rows)
    this.activeSurfaceHandle = handle
    this.attachedSessionId = sessionId

    try {
      const updated = await this.client.terminalAttach(
        sessionId,
        this.lastCols,
        this.lastRows,
        (event: TerminalStreamEvent) => {
          if (!this.active || this.attachedSessionId !== sessionId) return
          this.handleStreamEvent(sessionId, event)
        },
      )
      if (!this.active) return
      this.upsertSession(updated)
    } catch (err) {
      if (this.active) {
        this.error = err instanceof Error ? err.message : String(err)
      }
    }
  }

  /** Detaches the UI stream for `sessionId` while keeping the `herdr-core` session alive in the background. */
  async detachSurface(sessionId: string): Promise<void> {
    if (this.attachedSessionId === sessionId) {
      this.attachedSessionId = null
      this.activeSurfaceHandle = null
    }
    if (!this.client.terminalDetach) return
    try {
      await this.client.terminalDetach(sessionId)
    } catch {
      // Idempotent detach.
    }
  }

  /** Sends raw keyboard or paste bytes to `sessionId`. */
  async writeInput(sessionId: string, bytes: Uint8Array): Promise<void> {
    if (!this.client.terminalWrite || bytes.length === 0) return
    try {
      await this.client.terminalWrite(sessionId, Array.from(bytes))
    } catch {
      // Ignore transient write errors during detach.
    }
  }

  /** Sends a grid resize to `sessionId`. */
  async resize(sessionId: string, cols: number, rows: number): Promise<void> {
    const nextCols = Math.max(20, cols)
    const nextRows = Math.max(4, rows)
    this.lastCols = nextCols
    this.lastRows = nextRows
    if (!this.client.terminalResize || this.attachedSessionId !== sessionId) return
    try {
      await this.client.terminalResize(sessionId, nextCols, nextRows)
    } catch {
      // Ignore transient resize errors.
    }
  }

  /** Marks any unseen `done` completion on `sessionId` as seen (`blue -> green`). */
  async markSeen(sessionId: string): Promise<void> {
    if (!this.client.terminalMarkSeen) return
    try {
      const updated = await this.client.terminalMarkSeen(sessionId)
      if (updated && this.active) {
        this.upsertSession(updated)
      }
    } catch {
      // Ignore transient mark-seen errors.
    }
  }

  /** Focuses the currently mounted `TerminalSurface`, if any. */
  focusActiveSurface(): void {
    this.activeSurfaceHandle?.focus()
  }

  /** Refits the currently mounted `TerminalSurface` to its container dimensions. */
  fitActiveSurface(): void {
    this.activeSurfaceHandle?.fit()
  }

  /** Jumps to `sessionId`: switches workspace if needed, opens `RightDock`, activates its tab, marks seen, and focuses the surface. */
  async jumpToSession(sessionId: string): Promise<void> {
    const target = this.sessions.find((s) => s.id === sessionId)
    if (!target) return
    this.activeTabByWorkspace = {
      ...this.activeTabByWorkspace,
      [target.workspaceRoot]: target.id,
    }
    this.peers.openRightDock()
    if (this.peers.getRoot() !== target.workspaceRoot) {
      await this.peers.selectWorkspace(target.workspaceRoot)
    }
    await this.markSeen(target.id)
    requestAnimationFrame(() => {
      this.focusActiveSurface()
    })
  }

  /** Handles a host-pushed `terminal-session-updated` event for reactive dots, `AgentsPane`, and background toasts. */
  handleSessionUpdated(session: TerminalSession): void {
    if (!this.active) return
    const previous = this.sessions.find((s) => s.id === session.id)
    this.upsertSession(session)

    const isFocusedTab =
      this.peers.isRightDockOpen() &&
      this.peers.getRoot() === session.workspaceRoot &&
      this.activeSessionId === session.id

    if (isFocusedTab) return

    const attentionStatus: 'done' | 'blocked' | null =
      session.status === 'done' || session.status === 'blocked' ? session.status : null
    const transitionedToAttention =
      attentionStatus !== null &&
      previous?.status !== attentionStatus &&
      this.notifiedSeqBySession.get(session.id) !== session.stateChangeSeq

    if (transitionedToAttention && attentionStatus !== null) {
      this.notifiedSeqBySession.set(session.id, session.stateChangeSeq)
      const workspaceName = session.workspaceRoot.split('/').filter(Boolean).at(-1) ?? 'workspace'
      const actor = session.agent?.label ?? session.title
      const verb = attentionStatus === 'blocked' ? 'needs attention' : 'finished'
      this.peers.onNotification?.({
        sessionId: session.id,
        workspaceRoot: session.workspaceRoot,
        status: attentionStatus,
        title: `${actor} ${verb}`,
        message: session.agent?.message
          ? `${workspaceName}: ${session.agent.message}`
          : workspaceName,
      })
    }
  }

  /** Handles a host-pushed `terminal-exited` event. */
  handleExited(event: TerminalExitEvent): void {
    if (!this.active) return
    this.removeSession(event.sessionId)
  }

  /** Marks this service inactive and detaches any active UI stream on unmount. */
  dispose(): void {
    const current = this.attachedSessionId
    this.active = false
    this.attachedSessionId = null
    this.activeSurfaceHandle = null
    if (current) {
      void this.client.terminalDetach?.(current)
    }
  }

  // Private -------------------------------------------------------------------

  private handleStreamEvent(sessionId: string, event: TerminalStreamEvent): void {
    switch (event.kind) {
      case 'replay':
      case 'output':
        if (event.data.length > 0) {
          this.activeSurfaceHandle?.write(new Uint8Array(event.data))
        }
        break
      case 'sessionUpdated':
        this.handleSessionUpdated(event.session)
        break
      case 'exited':
        this.removeSession(sessionId)
        break
    }
  }

  private upsertSession(next: TerminalSession): void {
    if (!next.alive) {
      this.removeSession(next.id)
      return
    }
    const idx = this.sessions.findIndex((s) => s.id === next.id)
    if (idx >= 0) {
      this.sessions = [...this.sessions.slice(0, idx), next, ...this.sessions.slice(idx + 1)]
    } else {
      this.sessions = [...this.sessions, next]
    }
  }

  private removeSession(sessionId: string): void {
    const removed = this.sessions.find((s) => s.id === sessionId)
    this.sessions = this.sessions.filter((s) => s.id !== sessionId)
    if (removed && this.activeTabByWorkspace[removed.workspaceRoot] === sessionId) {
      const remaining = this.sessions.filter((s) => s.workspaceRoot === removed.workspaceRoot)
      const nextMap = { ...this.activeTabByWorkspace }
      if (remaining.length > 0) {
        nextMap[removed.workspaceRoot] = remaining[0].id
      } else {
        delete nextMap[removed.workspaceRoot]
      }
      this.activeTabByWorkspace = nextMap
    }
    if (this.attachedSessionId === sessionId) {
      this.attachedSessionId = null
      this.activeSurfaceHandle = null
    }
  }
}
