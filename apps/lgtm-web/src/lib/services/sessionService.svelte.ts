import { tick } from 'svelte'
import type { TabItem } from 'lgtm-ui'
import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
import {
  errorMessage,
  type Session,
  type SidebarWidget,
  type TabCandidate,
  type WorkspaceClient,
  type WorkspaceNode,
} from '../clients/workspaceClient'
import type { CachedWorkspaceSnapshot, WorkspaceTab } from '../types'
import type { LspService } from './lspService.svelte'
import type { ProjectService } from './projectService.svelte'
import type { SettingsService } from './settingsService.svelte'
import type { WorkspaceService } from './workspaceService.svelte'

export { type CachedWorkspaceSnapshot }

/** Peer services coordinated by `SessionService` during workspace switching and session hydration. */
export interface SessionServicePeers {
  workspace: WorkspaceService
  project: ProjectService
  lsp: LspService
  settings: SettingsService
}

/** Frontend session service managing active workspace switching, left dock layout, open tabs, snapshot caching, and `~/.lgtm/sessions/<id>.json` persistence. */
export class SessionService {
  root = $state('~/.lgtm/workspace')
  defaultRoot = $state('')
  sidebarWidget = $state<SidebarWidget>('explorer')
  projectsOpen = $state(false)
  explorerOpen = $state(true)
  projectsWidth = $state(180)
  widgetWidth = $state(236)
  resizing = $state(false)
  tabs = $state<WorkspaceTab[]>([])
  activeTabId = $state<string | null>(null)
  private active = true
  private workspaceCache = new Map<string, CachedWorkspaceSnapshot>()
  private sessionHydratedRoots = new Set<string>()
  private sidebarHydrated = false
  private sessionSaveQueue: Promise<void> = Promise.resolve()
  private peers: SessionServicePeers | null = null

  /** Creates a `SessionService` bound to `client`. */
  constructor(private client: WorkspaceClient) {}

  // Public --------------------------------------------------------------------

  /** Binds peer domain services after construction so workspace switching and tab persistence coordinate cleanly without circular ownership. */
  bindPeers(peers: SessionServicePeers): void {
    this.peers = peers
  }

  /** Returns whether this service instance is still mounted and active. */
  get isActive(): boolean {
    return this.active
  }

  /** Returns the total rendered width (in px) of the open left-dock panes (`ProjectsPane` + `WidgetPane`). */
  get leftDockWidth(): number {
    return (this.projectsOpen ? this.projectsWidth : 0) + (this.explorerOpen ? this.widgetWidth : 0)
  }

  /** Returns the currently active editor or diagnostics tab, if any. */
  get activeTab(): WorkspaceTab | null {
    if (!this.activeTabId) return null
    return this.tabs.find((tab) => tab.id === this.activeTabId) ?? null
  }

  /** Returns tab bar items formatted for the Helical `Tabs` component. */
  get tabItems(): TabItem[] {
    return this.tabs.map((tab) => ({
      value: tab.id,
      label: tab.title,
      dirty: Boolean(tab.dirty),
      closable: true,
    }))
  }

  /** Clamps and updates the width of the `Projects` dock pane. */
  setProjectsWidth(width: number): void {
    this.projectsWidth = Math.min(360, Math.max(140, Math.round(width)))
  }

  /** Clamps and updates the width of the `WidgetPane` (`FileExplorer`). */
  setWidgetWidth(width: number): void {
    this.widgetWidth = Math.min(480, Math.max(160, Math.round(width)))
  }

  /** Updates whether a left-dock pane resizer is actively being dragged. */
  setResizing(active: boolean): void {
    this.resizing = active
  }

  /** Toggles the Projects dock pane independently of the Explorer widget slot and shifts focus to the opened pane or editor on close. */
  toggleProjectsPane(): void {
    this.projectsOpen = !this.projectsOpen
    this.sidebarWidget = this.projectsOpen ? 'projects' : 'explorer'
    this.sidebarHydrated = true
    void this.saveCurrentWorkspaceSession()
    this.focusTargetPane(this.projectsOpen ? 'projects' : 'editor')
  }

  /** Toggles the Explorer widget pane inside the left dock independently of the Projects pane and shifts focus to the opened pane or editor on close. */
  toggleExplorerPane(): void {
    this.explorerOpen = !this.explorerOpen
    this.sidebarHydrated = true
    void this.saveCurrentWorkspaceSession()
    this.focusTargetPane(this.explorerOpen ? 'workspace' : 'editor')
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
    void this.saveCurrentWorkspaceSession()
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

  /** Applies an incoming `Session` snapshot from the backend to `ProjectService` and dock visibility. */
  applySession(session: Session): void {
    if (!this.active) return
    this.peers?.project.applySessionProjects(session)
    this.hydrateSidebarFromSession(session)
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

  /** Synchronizes session state with the backend and optionally hydrates persisted tabs and expanded dirs for `this.root`. */
  async syncSession(hydrateTabs = false): Promise<void> {
    const session = await this.fetchSession()
    if (!session || !this.active) return
    this.applySession(session)
    const workspace = this.peers?.workspace
    if (hydrateTabs && workspace && !this.hasHydratedRoot(this.root)) {
      this.markHydratedRoot(this.root)
      const wsState = session.workspaces?.[this.root]
      if (wsState) {
        const dirs = [...(wsState.expandedDirs ?? [])]
        await workspace.preloadExpandedDirs(dirs)
        if (!this.active) return
        workspace.setHydratedExpandedDirs(dirs)
        if (this.tabs.length === 0 && (wsState.tabs ?? []).length > 0) {
          await this.restorePersistedTabs(wsState.tabs)
        }
      }
    }
  }

  /** Opens the default or specified workspace directory and refreshes settings, session, file tree, and LSP state. */
  async openWorkspace(path?: string): Promise<void> {
    const peers = this.peers
    if (!peers) return
    const { workspace, lsp, settings } = peers
    const outgoingRoot = this.root && !this.isDefaultPlaceholderRoot(this.root) ? this.root : null
    if (outgoingRoot && path && path !== outgoingRoot) {
      this.saveCurrentWorkspaceToCache(outgoingRoot)
    }
    await workspace.flushSave()
    if (outgoingRoot && path && path !== outgoingRoot) {
      void this.saveCurrentWorkspaceSession(outgoingRoot)
    }

    const cached = path ? this.getCachedWorkspace(path) : undefined
    if (cached && path) {
      this.applyCachedWorkspace(cached)
      void this.refreshWorkspaceInBackground(path)
      return
    }

    const seqBeforeOpen = workspace.getOpenRequestSeq()
    workspace.beginWorkspaceOpen()
    try {
      const result = await this.client.open(path)
      if (!this.active) return
      if (!this.defaultRoot && !path && this.isDefaultWorkspaceRoot(result.root)) {
        this.defaultRoot = result.root
      }
      const switchedCached = this.getCachedWorkspace(result.root)
      const fileOpenedDuringInit = !path && workspace.getOpenRequestSeq() !== seqBeforeOpen
      this.root = result.root
      settings.rememberRoot(result.root)
      if (switchedCached) {
        this.tabs = switchedCached.tabs
        this.activeTabId = switchedCached.activeTabId
        workspace.applyCachedWorkspace(switchedCached, result.entries)
      } else {
        if (!fileOpenedDuringInit) {
          this.tabs = []
          this.activeTabId = null
        }
        workspace.applyOpenedListing(result.entries, fileOpenedDuringInit)
      }
      if (!fileOpenedDuringInit) {
        lsp.reset()
        workspace.bumpFileVersion()
      }
      workspace.bumpTreeVersion()
      void settings.load()
      void lsp.syncState()
      await this.syncSession(!switchedCached && !fileOpenedDuringInit)
      this.saveCurrentWorkspaceToCache(this.root)
    } catch (error) {
      if (this.active) workspace.error = errorMessage(error)
    } finally {
      if (this.active) workspace.loading = false
    }
  }

  /** Switches to a selected workspace root, opens the native folder picker, or opens the `New Worktree` dialog. */
  async selectWorkspace(value: string): Promise<void> {
    const workspace = this.peers?.workspace
    if (workspace?.blocked) return
    if (value === '__new_worktree__') {
      this.peers?.project.openNewWorktreeDialog()
      return
    }
    if (value === '__open_folder__') {
      try {
        const chosen = await this.client.pickFolder()
        if (chosen && this.active) {
          await this.openWorkspace(chosen)
        }
      } catch (error) {
        if (this.active && workspace) workspace.creationError = errorMessage(error)
      }
      return
    }
    if (value && value !== this.root) {
      await this.openWorkspace(value)
    }
  }

  /** Activates an open file or diagnostics tab by its ID. */
  async activateTab(tabId: string): Promise<void> {
    const peers = this.peers
    if (!peers || peers.workspace.blocked || this.activeTabId === tabId) return
    const tab = this.tabs.find((item) => item.id === tabId)
    if (!tab) return
    if (tab.kind === 'file' && tab.path) {
      await peers.workspace.openFile(
        {
          path: tab.path,
          name: tab.title,
          kind: 'file',
        },
        undefined,
        true,
      )
      return
    }
    await peers.workspace.flushSave()
    this.activeTabId = tab.id
    peers.workspace.showDiagnosticsTabSnapshot(tab.content, tab.languageId ?? null)
    try {
      const refreshed = await peers.lsp.getDiagnosticsBuffer(tab.languageId ?? undefined)
      if (!this.active) return
      this.tabs = this.tabs.map((item) =>
        item.id === tab.id ? { ...item, title: refreshed.title, content: refreshed.content } : item,
      )
      if (this.activeTabId === tab.id) {
        peers.workspace.updateDiagnosticsContent(refreshed.content, refreshed.languageId ?? null)
      }
    } catch {
      // Keep existing diagnostics buffer snapshot if refresh is unsupported.
    }
  }

  /** Flushes pending saves and closes the tab with `tabId`. */
  async closeTab(tabId: string): Promise<void> {
    const workspace = this.peers?.workspace
    if (!workspace || workspace.blocked) return
    const index = this.tabs.findIndex((item) => item.id === tabId)
    if (index === -1) return
    await workspace.flushSave()
    const remaining = this.tabs.filter((item) => item.id !== tabId)
    this.tabs = remaining
    if (this.activeTabId !== tabId) {
      this.saveCurrentWorkspaceToCache(this.root)
      void this.saveCurrentWorkspaceSession()
      return
    }
    const fallback = remaining[index] ?? remaining[index - 1] ?? null
    if (!fallback) {
      this.activeTabId = null
      workspace.clearSelected()
      this.saveCurrentWorkspaceToCache(this.root)
      void this.saveCurrentWorkspaceSession()
      return
    }
    this.activeTabId = null
    await this.activateTab(fallback.id)
  }

  /** Closes the currently active editor tab, if any. */
  async closeActiveTab(): Promise<void> {
    if (!this.activeTabId) return
    await this.closeTab(this.activeTabId)
  }

  /** Cycles forward through open tabs by `count`. */
  async selectNextTab(count = 1): Promise<void> {
    if (this.peers?.workspace.blocked || this.tabs.length <= 1) return
    const currentIdx = this.tabs.findIndex((tab) => tab.id === this.activeTabId)
    const startIdx = currentIdx === -1 ? 0 : currentIdx
    const step = Math.max(1, count)
    const target = this.tabs[(startIdx + step) % this.tabs.length]
    if (target) {
      await this.activateTab(target.id)
    }
  }

  /** Cycles backward through open tabs by `count`. */
  async selectPrevTab(count = 1): Promise<void> {
    if (this.peers?.workspace.blocked || this.tabs.length <= 1) return
    const currentIdx = this.tabs.findIndex((tab) => tab.id === this.activeTabId)
    const startIdx = currentIdx === -1 ? 0 : currentIdx
    const step = Math.max(1, count)
    const nextIdx = (((startIdx - step) % this.tabs.length) + this.tabs.length) % this.tabs.length
    const target = this.tabs[nextIdx]
    if (target) {
      await this.activateTab(target.id)
    }
  }

  /** Inserts or replaces `tab` in the open tabs list and activates it. */
  upsertAndActivateTab(tab: WorkspaceTab): void {
    const existingIdx = this.tabs.findIndex((item) => item.id === tab.id)
    if (existingIdx === -1) {
      this.tabs = [...this.tabs, tab]
    } else {
      this.tabs = this.tabs.map((item, idx) => (idx === existingIdx ? tab : item))
    }
    this.activeTabId = tab.id
  }

  /** Marks the file tab for `path` dirty and edited in memory and refreshes the session cache. */
  markTabDirty(path: string, content: string): void {
    const tabId = `file:${path}`
    this.tabs = this.tabs.map((tab) =>
      tab.id === tabId ? { ...tab, content, dirty: true, edited: true } : tab,
    )
    this.saveCurrentWorkspaceToCache(this.root)
  }

  /** Marks the file tab for `path` clean (`dirty: false`, `edited: true`) when its saved content matches `content`. */
  markTabSaved(path: string, content: string): void {
    const tabId = `file:${path}`
    this.tabs = this.tabs.map((tab) =>
      tab.id === tabId && tab.content === content ? { ...tab, dirty: false, edited: true } : tab,
    )
    this.saveCurrentWorkspaceToCache(this.root)
    void this.saveCurrentWorkspaceSession()
  }

  /** Updates matching file tab content after reloading a clean `config.json` tab from disk. */
  updateCleanTabContent(tabId: string, content: string): void {
    this.tabs = this.tabs.map((item) =>
      item.id === tabId && !item.dirty ? { ...item, content } : item,
    )
  }

  /** Updates tab IDs, paths, and titles affected by renaming a file or directory. */
  handlePathRenamed(oldPath: string, renamed: WorkspaceNode): void {
    this.tabs = this.tabs.map((tab) => {
      if (tab.kind !== 'file' || !tab.path) return tab
      if (tab.path === oldPath) {
        return {
          ...tab,
          id: `file:${renamed.path}`,
          path: renamed.path,
          title: renamed.name,
        }
      }
      if (tab.path.startsWith(`${oldPath}/`)) {
        const nextPath = renamed.path + tab.path.slice(oldPath.length)
        return {
          ...tab,
          id: `file:${nextPath}`,
          path: nextPath,
        }
      }
      return tab
    })
  }

  /** Removes open file tabs located at or inside a deleted `path`. */
  handlePathsRemoved(path: string): void {
    this.tabs = this.tabs.filter(
      (tab) =>
        tab.kind !== 'file' || !tab.path || (tab.path !== path && !tab.path.startsWith(`${path}/`)),
    )
  }

  /** Stores the active workspace's in-memory snapshot in the session cache. */
  saveCurrentWorkspaceToCache(root = this.root): void {
    if (!root || this.isDefaultPlaceholderRoot(root)) return
    const snapshot = this.buildSnapshotForRoot(root)
    if (snapshot) {
      this.setCachedWorkspace(root, snapshot)
    }
  }

  /** Persists the active workspace's filtered tab candidates (`edited || lastOpened`), expanded directories, and dock widget to `~/.lgtm/sessions/<id>.json`. */
  async saveCurrentWorkspaceSession(targetRoot = this.root): Promise<void> {
    const isPlaceholder = !targetRoot || this.isDefaultPlaceholderRoot(targetRoot)
    const snapshot = isPlaceholder
      ? undefined
      : targetRoot === this.root
        ? this.buildSnapshotForRoot(targetRoot)
        : this.getCachedWorkspace(targetRoot)
    await this.saveWorkspaceSession(isPlaceholder ? '' : targetRoot, snapshot, (session) =>
      this.applySession(session),
    )
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

  /** Persists `snapshot` and dock widget state via `session_save`. */
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

  /** Flushes session persistence and marks this service inactive on teardown. */
  dispose(): void {
    void this.saveCurrentWorkspaceSession()
    this.active = false
  }

  // Private -------------------------------------------------------------------

  private isDefaultPlaceholderRoot(path: string): boolean {
    return path === '~/.lgtm/workspace'
  }

  private isDefaultWorkspaceRoot(path: string): boolean {
    return (
      path === '~/.lgtm/workspace' ||
      path.endsWith('/.lgtm/workspace') ||
      path.endsWith('\\.lgtm\\workspace')
    )
  }

  private buildSnapshotForRoot(root: string): CachedWorkspaceSnapshot | undefined {
    const workspace = this.peers?.workspace
    if (!workspace) return undefined
    const retainedTabs = this.tabs.filter(
      (tab) =>
        tab.kind === 'file' &&
        Boolean(tab.path) &&
        (Boolean(tab.edited) || tab.id === this.activeTabId),
    )
    const activeTab =
      retainedTabs.find((tab) => tab.id === this.activeTabId) ?? retainedTabs.at(-1) ?? null
    return workspace.buildSnapshot(root, retainedTabs, activeTab)
  }

  private applyCachedWorkspace(cached: CachedWorkspaceSnapshot): void {
    this.root = cached.root
    this.tabs = cached.tabs
    this.activeTabId = cached.activeTabId
    this.peers?.workspace.applyCachedWorkspace(cached)
    this.peers?.lsp.reset()
    this.peers?.workspace.bumpTreeVersion()
    this.peers?.workspace.bumpFileVersion()
  }

  private async refreshWorkspaceInBackground(path: string): Promise<void> {
    const peers = this.peers
    if (!peers) return
    const { workspace, settings, lsp } = peers
    try {
      const result = await this.client.open(path)
      if (!this.active || this.root !== path) return
      this.root = result.root
      settings.rememberRoot(result.root)
      workspace.setEntries(result.entries)
      void settings.load()
      void lsp.syncState()
      await this.syncSession(false)
      this.saveCurrentWorkspaceToCache(this.root)
    } catch (error) {
      if (this.active && this.root === path) {
        workspace.error = errorMessage(error)
      }
    }
  }

  private async restorePersistedTabs(
    persistedTabs: { path: string; title: string; edited: boolean; lastOpened: boolean }[],
  ): Promise<void> {
    const restored: WorkspaceTab[] = []
    let activeId: string | null = null
    let activeEntry: WorkspaceNode | null = null
    let activeContent: CachedWorkspaceSnapshot['fileContent'] = null

    for (const item of persistedTabs) {
      try {
        const content = await this.client.read(item.path)
        if (!this.active) return
        const tabId = `file:${item.path}`
        const tab: WorkspaceTab = {
          id: tabId,
          kind: 'file',
          title: item.title || (item.path.split('/').at(-1) ?? item.path),
          path: item.path,
          languageId: content.languageId ?? null,
          content: content.content,
          readonly: false,
          dirty: false,
          edited: Boolean(item.edited),
        }
        restored.push(tab)
        if (item.lastOpened || !activeId) {
          activeId = tabId
          activeEntry = { path: item.path, name: tab.title, kind: 'file' }
          activeContent = content
        }
      } catch {
        // Skip persisted tabs whose files were deleted outside the session.
      }
    }

    if (restored.length > 0 && this.active) {
      this.tabs = restored
      this.activeTabId = activeId
      this.peers?.workspace.applyRestoredBuffer(activeEntry, activeContent)
      this.saveCurrentWorkspaceToCache(this.root)
    }
  }

  private focusTargetPane(paneId: 'projects' | 'workspace' | 'editor'): void {
    if (typeof document === 'undefined') return
    void tick().then(() => {
      requestAnimationFrame(() => {
        getDocumentWhichKeyController(document)?.focusPane(paneId)
      })
    })
  }
}
