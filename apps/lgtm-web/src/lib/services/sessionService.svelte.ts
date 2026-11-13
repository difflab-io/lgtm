import type {
  Session,
  SidebarWidget,
  TabCandidate,
  WorkspaceClient,
} from '../clients/workspaceClient'
import type { CachedWorkspaceSnapshot } from '../types'

export { type CachedWorkspaceSnapshot }

/** Frontend session service managing left-dock visibility, resizable pane widths, per-workspace cache snapshots, and `~/.lgtm/sessions/<id>.json` persistence. */
export class SessionService {
  sidebarWidget = $state<SidebarWidget>('explorer')
  projectsOpen = $state(false)
  explorerOpen = $state(true)
  projectsWidth = $state(180)
  widgetWidth = $state(236)
  private workspaceCache = new Map<string, CachedWorkspaceSnapshot>()
  private sessionHydratedRoots = new Set<string>()
  private sidebarHydrated = false
  private sessionSaveQueue: Promise<void> = Promise.resolve()

  /** Creates a `SessionService` bound to `client` and an optional callback invoked when dock visibility changes. */
  constructor(
    private client: WorkspaceClient,
    private onDockChanged?: () => void,
  ) {}

  // Public --------------------------------------------------------------------

  /** Returns the total rendered width (in px) of the open left-dock panes (`ProjectsPane` + `WidgetPane`). */
  get leftDockWidth(): number {
    return (this.projectsOpen ? this.projectsWidth : 0) + (this.explorerOpen ? this.widgetWidth : 0)
  }

  /** Clamps and updates the width of the `Projects` dock pane. */
  setProjectsWidth(width: number): void {
    this.projectsWidth = Math.min(360, Math.max(140, Math.round(width)))
  }

  /** Clamps and updates the width of the `WidgetPane` (`FileExplorer`). */
  setWidgetWidth(width: number): void {
    this.widgetWidth = Math.min(480, Math.max(160, Math.round(width)))
  }

  /** Toggles the Projects dock pane independently of the Explorer widget slot. */
  toggleProjectsPane(): void {
    this.projectsOpen = !this.projectsOpen
    this.sidebarWidget = this.projectsOpen ? 'projects' : 'explorer'
    this.sidebarHydrated = true
    this.onDockChanged?.()
  }

  /** Toggles the Explorer widget pane inside the left dock independently of the Projects pane. */
  toggleExplorerPane(): void {
    this.explorerOpen = !this.explorerOpen
    this.sidebarHydrated = true
    this.onDockChanged?.()
  }

  /** Opens the specified left dock pane (`projects` or `explorer`). */
  setSidebarWidget(widget: SidebarWidget): void {
    if (widget === 'projects') {
      this.projectsOpen = true
      this.sidebarWidget = 'projects'
    } else {
      this.explorerOpen = true
      if (!this.projectsOpen) {
        this.sidebarWidget = 'explorer'
      }
    }
    this.sidebarHydrated = true
    this.onDockChanged?.()
  }

  /** Hydrates initial sidebar dock visibility from `session` if not yet modified by the user. */
  hydrateSidebarFromSession(session: Session): void {
    if (!this.sidebarHydrated) {
      this.sidebarHydrated = true
      const nextWidget = session.sidebarWidget ?? 'explorer'
      this.sidebarWidget = nextWidget
      this.projectsOpen = nextWidget === 'projects'
    }
  }

  /** Returns the cached in-memory snapshot for `root`, if present. */
  getCachedWorkspace(root: string): CachedWorkspaceSnapshot | undefined {
    return this.workspaceCache.get(root)
  }

  /** Stores an in-memory snapshot for `root`. */
  setCachedWorkspace(root: string, snapshot: CachedWorkspaceSnapshot): void {
    this.workspaceCache.set(root, snapshot)
  }

  /** Removes cached workspace state and hydration tracking for `root`. */
  evictWorkspace(root: string): void {
    this.workspaceCache.delete(root)
    this.sessionHydratedRoots.delete(root)
  }

  /** Returns whether persisted tabs and expanded directories have already been hydrated for `root`. */
  hasHydratedRoot(root: string): boolean {
    return this.sessionHydratedRoots.has(root)
  }

  /** Marks `root` as hydrated for the current session. */
  markHydratedRoot(root: string): void {
    this.sessionHydratedRoots.add(root)
  }

  /** Fetches the persisted `Session` from the backend, if supported by `client`. */
  async fetchSession(): Promise<Session | null> {
    if (!this.client.sessionGet) return null
    try {
      return await this.client.sessionGet()
    } catch {
      return null
    }
  }

  /** Persists the workspace's filtered tab candidates (`edited || lastOpened`), expanded directories, and active dock widget. */
  async saveWorkspaceSession(
    workspaceRoot: string,
    snapshot: CachedWorkspaceSnapshot | undefined,
    onSessionUpdated?: (session: Session) => void,
  ): Promise<void> {
    const saveFn = this.client.sessionSave
    if (!saveFn) return
    const candidates: TabCandidate[] = snapshot
      ? snapshot.tabs
          .filter((tab) => tab.kind === 'file' && Boolean(tab.path))
          .map((tab) => ({
            path: tab.path!,
            title: tab.title,
            edited: Boolean(tab.edited),
            lastOpened: tab.id === snapshot.activeTabId,
          }))
      : []
    const task = this.sessionSaveQueue.then(async () => {
      try {
        const session = await saveFn({
          workspaceRoot,
          tabs: candidates,
          expandedDirs: snapshot ? snapshot.expandedDirs : [],
          sidebarWidget: this.sidebarWidget,
        })
        onSessionUpdated?.(session)
      } catch {
        // Ignore background session save failures.
      }
    })
    this.sessionSaveQueue = task.catch(() => {})
    await task
  }
}
