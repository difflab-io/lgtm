import { tick } from 'svelte'
import type {
  TreeNode,
  ContextMenuItem,
  CodeEditorCompletionItem,
  CodeEditorHoverResult,
  CodeEditorSignatureHelp,
  TabItem,
} from 'lgtm-ui'
import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
import { WorkspaceNodeViewModel } from '../models/workspaceNodeViewModel'
import {
  errorMessage,
  type WorkspaceClient,
  type WorkspaceNode,
  type FileContent,
  type RightDockWidget,
  type Session,
  type SidebarWidget,
  type TabCandidate,
  type WorkspaceSessionState,
  type WorkspaceStatePatch,
} from '../clients/workspaceClient'
import type { CachedWorkspaceSnapshot, FolderRefresh, WorkspaceTab } from '../types'
import type { LspService } from './lspService.svelte'
import type { SessionService } from './sessionService.svelte'
import type { SettingsService } from './settingsService.svelte'

/** Peer callbacks invoked by `WorkspaceService` for shell/workspace actions crossing sibling domains. */
export interface WorkspaceServiceCallbacks {
  onNewWorktreeRequested?: () => void
}

/** Active workspace & shell state service managing open tabs, dock widgets/widths, workspace switching, file explorer tree, and active buffer I/O. */
export class WorkspaceService {
  defaultRoot = $state('')
  root = $state('~/.lgtm/workspace')
  sidebarWidget = $state<SidebarWidget>('explorer')
  projectsOpen = $state(false)
  explorerOpen = $state(true)
  projectsWidth = $state(180)
  widgetWidth = $state(236)
  rightDockOpen = $state(false)
  rightDockWidget = $state<RightDockWidget>('terminal')
  rightDockWidth = $state(420)
  agentsPaneHeight = $state(180)
  resizing = $state(false)
  tabs = $state<WorkspaceTab[]>([])
  activeTabId = $state<string | null>(null)
  entries = $state<WorkspaceNode[]>([])
  loading = $state(true)
  error = $state<string | null>(null)
  creationError = $state<string | null>(null)
  pendingEntry = $state<WorkspaceNode | null>(null)
  pendingParent = $state('')
  refreshFolder = $state<FolderRefresh | undefined>()
  treeVersion = $state(0)
  busy = $state(false)
  selected = $state<WorkspaceNode | null>(null)
  fileContent = $state<FileContent | null>(null)
  fileLoading = $state(false)
  saving = $state(false)
  openingError = $state<string | null>(null)
  fileVersion = $state(0)
  focusRequest = $state(0)
  selectionOffset = $state<number | null>(null)
  expandedDirs = $state<string[]>([])
  private dirEntries = $state<Record<string, WorkspaceNode[]>>({})
  private listed = $state<Record<string, WorkspaceNode>>({})
  private workspaceCache = new Map<string, CachedWorkspaceSnapshot>()
  private sidebarHydrated = false
  private active = true
  private dirtyPath: string | null = null
  private dirtyContent: string | null = null
  private saveTimer: ReturnType<typeof setTimeout> | null = null
  private inFlightSave: Promise<void> | null = null
  private openRequestSeq = 0

  /** Constructs a `WorkspaceService` owning shell/workspace UI state backed by `client` and peer services. */
  constructor(
    readonly client: WorkspaceClient,
    private readonly session: SessionService,
    private readonly lsp: LspService,
    private readonly settings: SettingsService,
    private readonly callbacks: WorkspaceServiceCallbacks = {},
  ) {}

  // Public --------------------------------------------------------------------

  /** Returns the total rendered width (in px) of the open left-dock panes (`ProjectsPane` + `WidgetPane`). */
  get leftDockWidth(): number {
    return (this.projectsOpen ? this.projectsWidth : 0) + (this.explorerOpen ? this.widgetWidth : 0)
  }

  /** Returns whether a file or tree operation is currently in progress. */
  get blocked(): boolean {
    return this.busy || this.fileLoading
  }

  /** Returns whether creating a new file or folder is currently disabled. */
  get cannotCreate(): boolean {
    return this.blocked || this.loading || Boolean(this.error || this.pendingEntry)
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

  /** Returns the display title of the active tab or selected file. */
  get selectedTitle(): string {
    if (this.activeTab?.kind === 'diagnostics') {
      return this.activeTab.title
    }
    return this.selected ? this.selected.name : 'No file selected'
  }

  /** Returns the path or title shown in the footer breadcrumb tooltip. */
  get selectedPath(): string | undefined {
    if (this.activeTab?.kind === 'diagnostics') return this.activeTab.title
    return this.selected?.path
  }

  /** Returns the filename passed to `CodeEditor` for syntax highlighting. */
  get activeEditorFilename(): string {
    if (this.activeTab?.kind === 'diagnostics') return 'diagnostics.md'
    if (this.selected?.path.endsWith('config.json')) return 'config.json'
    return this.selected?.name ?? 'diagnostics.md'
  }

  /** Returns path segments rendered in the footer breadcrumb bar. */
  get breadcrumbs(): string[] {
    if (this.activeTab?.kind === 'diagnostics') return [this.activeTab.title]
    if (!this.selected) return []
    if (this.isConfigFilePath(this.selected.path)) {
      return ['.lgtm', 'config.json']
    }
    return this.selected.path.split(/[\\/]/).filter(Boolean)
  }

  /** Returns the language ID of the active editor buffer. */
  get activeLanguageId(): string | null {
    if (this.activeTab?.kind === 'diagnostics') {
      return this.activeTab.languageId ?? null
    }
    return this.fileContent?.languageId ?? null
  }

  /** Returns the human-readable editor status label for the footer. */
  get status(): string {
    if (this.openingError) return this.openingError
    if (this.fileLoading) return 'Opening file…'
    if (this.saving) return 'Saving…'
    if (this.selected || this.activeTab) return 'Ready'
    return 'Idle'
  }

  /** Returns top-level tree nodes for the sidebar `TreeView`. */
  get nodes(): TreeNode[] {
    return [
      ...(this.pendingEntry && !this.pendingParent
        ? [new WorkspaceNodeViewModel(this.pendingEntry).toTreeNode()]
        : []),
      ...this.entries.map((entry) => this.toTreeNodeWithChildren(entry)),
    ]
  }

  /** Returns the pending inline folder creation node, if active. */
  get pendingNode(): TreeNode | undefined {
    return this.pendingEntry
      ? new WorkspaceNodeViewModel(this.pendingEntry).toTreeNode()
      : undefined
  }

  /** Clamps and updates the width of the `Projects` dock pane. */
  setProjectsWidth(width: number): void {
    this.projectsWidth = Math.min(360, Math.max(140, Math.round(width)))
  }

  /** Clamps and updates the width of the `WidgetPane` (`FileExplorer`). */
  setWidgetWidth(width: number): void {
    this.widgetWidth = Math.min(480, Math.max(160, Math.round(width)))
  }

  /** Clamps and updates the width of `RightDock`. */
  setRightDockWidth(width: number): void {
    this.rightDockWidth = Math.min(820, Math.max(240, Math.round(width)))
  }

  /** Clamps and updates the height of `AgentsPane` in the bottom section of `#projects-dock-pane`. */
  setAgentsPaneHeight(height: number): void {
    this.agentsPaneHeight = Math.min(420, Math.max(96, Math.round(height)))
  }

  /** Toggles `RightDock` open or closed for `widget` and persists the session-level layout state. */
  toggleRightDock(widget: RightDockWidget = this.rightDockWidget): void {
    if (this.rightDockOpen && this.rightDockWidget === widget) {
      this.rightDockOpen = false
      this.focusTargetPane('editor')
    } else {
      this.rightDockWidget = widget
      this.rightDockOpen = true
      this.focusTargetPane('right-dock')
    }
    this.sidebarHydrated = true
    void this.session.saveCurrentSession(this.root)
  }

  /** Opens `RightDock` on `widget` without collapsing if already open. */
  openRightDock(widget: RightDockWidget = 'terminal'): void {
    const changed = !this.rightDockOpen || this.rightDockWidget !== widget
    this.rightDockWidget = widget
    this.rightDockOpen = true
    this.sidebarHydrated = true
    if (changed) {
      void this.session.saveCurrentSession(this.root)
    }
    this.focusTargetPane('right-dock')
  }

  /** Persists updated dock dimensions after a pointer-drag or keyboard resize completes. */
  persistDockLayout(): void {
    void this.session.saveCurrentSession(this.root)
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
    void this.session.saveCurrentSession(this.root)
    this.focusTargetPane(this.projectsOpen ? 'projects' : 'editor')
  }

  /** Toggles the Explorer widget pane inside the left dock independently of the Projects pane and shifts focus to the opened pane or editor on close. */
  toggleExplorerPane(): void {
    this.explorerOpen = !this.explorerOpen
    this.sidebarHydrated = true
    void this.session.saveCurrentSession(this.root)
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
    void this.session.saveCurrentSession(this.root)
  }

  /** Hydrates initial sidebar and right-dock layout state from a loaded `Session` if not yet changed locally. */
  applySessionWidgets(session: Session): void {
    if (!this.sidebarHydrated) {
      this.sidebarHydrated = true
      const nextWidget = session.sidebarWidget ?? 'explorer'
      this.sidebarWidget = nextWidget
      this.projectsOpen = nextWidget === 'projects'
      this.rightDockOpen = Boolean(session.rightDockOpen)
      this.rightDockWidget = session.rightDockWidget ?? 'terminal'
      if (typeof session.rightDockWidth === 'number' && session.rightDockWidth >= 240) {
        this.rightDockWidth = session.rightDockWidth
      }
      if (typeof session.agentsPaneHeight === 'number' && session.agentsPaneHeight >= 96) {
        this.agentsPaneHeight = session.agentsPaneHeight
      }
    }
  }

  /** Restores expanded explorer branches and persisted editor tabs for the active workspace from `wsState`. */
  async restoreSessionWorkspaceState(wsState: WorkspaceSessionState | undefined): Promise<void> {
    if (!wsState || !this.active) return
    const dirs = [...(wsState.expandedDirs ?? [])]
    await this.preloadExpandedDirs(dirs)
    if (!this.active) return
    this.expandedDirs = dirs
    if (this.tabs.length === 0 && (wsState.tabs ?? []).length > 0) {
      await this.restorePersistedTabs(wsState.tabs)
    }
  }

  /** Builds a `WorkspaceStatePatch` capturing current open tabs, expanded dirs, and active dock widget for `SessionService`. */
  buildSessionSavePatch(targetRoot = this.root): WorkspaceStatePatch {
    const isPlaceholder = !targetRoot || this.isDefaultPlaceholderRoot(targetRoot)
    const snapshot = isPlaceholder
      ? undefined
      : targetRoot === this.root
        ? this.buildSnapshotForCurrentRoot()
        : this.workspaceCache.get(targetRoot)
    const tabs: TabCandidate[] = snapshot
      ? snapshot.tabs
          .filter((tab) => tab.kind === 'file' && Boolean(tab.path))
          .map((tab) => ({
            path: tab.path!,
            title: tab.title,
            edited: Boolean(tab.edited),
            lastOpened: tab.id === snapshot.activeTabId,
          }))
      : []
    return {
      workspaceRoot: isPlaceholder ? '' : targetRoot,
      tabs,
      expandedDirs: snapshot ? snapshot.expandedDirs : [],
      sidebarWidget: this.sidebarWidget,
      rightDockOpen: this.rightDockOpen,
      rightDockWidget: this.rightDockWidget,
      rightDockWidth: this.rightDockWidth,
      agentsPaneHeight: this.agentsPaneHeight,
    }
  }

  /** Removes any cached in-memory workspace snapshot for `root`. */
  evictCachedWorkspace(root: string): void {
    this.workspaceCache.delete(root)
  }

  /** Updates the tracked list of expanded directory paths in `FileExplorer` and persists session state. */
  setExpandedDirs(dirs: string[]): void {
    this.expandedDirs = [...dirs]
    void this.session.saveCurrentSession(this.root)
  }

  /** Opens the default or specified workspace directory and refreshes settings, session, and LSP state. */
  async open(path?: string): Promise<void> {
    const outgoingRoot = this.root && !this.isDefaultPlaceholderRoot(this.root) ? this.root : null
    if (outgoingRoot && path && path !== outgoingRoot) {
      this.saveCurrentWorkspaceToCache(outgoingRoot)
    }
    await this.flushSave()
    if (outgoingRoot && path && path !== outgoingRoot) {
      void this.session.saveCurrentSession(outgoingRoot)
    }

    const cached = path ? this.workspaceCache.get(path) : undefined
    if (cached && path) {
      this.applyCachedWorkspace(cached)
      void this.refreshWorkspaceInBackground(path)
      return
    }

    const seqBeforeOpen = this.openRequestSeq
    this.loading = true
    this.error = null
    this.creationError = null
    this.pendingEntry = null
    this.pendingParent = ''
    try {
      const result = await this.client.open(path)
      if (!this.active) return
      if (!this.defaultRoot && !path && this.isDefaultWorkspaceRoot(result.root)) {
        this.defaultRoot = result.root
      }
      const switchedCached = this.workspaceCache.get(result.root)
      const fileOpenedDuringInit = !path && this.openRequestSeq !== seqBeforeOpen
      this.root = result.root
      this.settings.rememberRoot(result.root)
      this.entries = result.entries
      if (switchedCached) {
        this.dirEntries = switchedCached.dirEntries
        this.listed = switchedCached.listed
        this.tabs = switchedCached.tabs
        this.activeTabId = switchedCached.activeTabId
        this.selected = switchedCached.selected
        this.fileContent = switchedCached.fileContent
        this.selectionOffset = switchedCached.selectionOffset
        this.expandedDirs = switchedCached.expandedDirs
      } else if (!fileOpenedDuringInit) {
        this.dirEntries = {}
        this.listed = {}
        this.tabs = []
        this.activeTabId = null
        this.expandedDirs = []
        this.clearSelected()
      }
      if (!fileOpenedDuringInit) {
        this.lsp.reset()
        this.fileVersion++
      }
      this.treeVersion++
      void this.settings.load()
      void this.lsp.syncState()
      await this.session.syncSession(!switchedCached && !fileOpenedDuringInit)
      this.saveCurrentWorkspaceToCache(this.root)
    } catch (error) {
      if (this.active) this.error = errorMessage(error)
    } finally {
      if (this.active) this.loading = false
    }
  }

  /** Switches to a selected workspace root, opens the native folder picker, or opens the `New Worktree` dialog. */
  async selectWorkspace(value: string): Promise<void> {
    if (this.blocked) return
    if (value === '__new_worktree__') {
      this.callbacks.onNewWorktreeRequested?.()
      return
    }
    if (value === '__open_folder__') {
      try {
        const chosen = await this.client.pickFolder()
        if (chosen && this.active) {
          await this.open(chosen)
        }
      } catch (error) {
        if (this.active) this.creationError = errorMessage(error)
      }
      return
    }
    if (value && value !== this.root) {
      await this.open(value)
    }
  }

  /** Lazily loads child nodes for an expanded directory in the sidebar tree. */
  async loadChildren(node: TreeNode): Promise<TreeNode[]> {
    if (node.value === this.pendingEntry?.path) return []
    const items = await this.client.list(node.value)
    if (!this.active) return []
    for (const entry of items) this.listed[entry.path] = entry
    this.dirEntries = { ...this.dirEntries, [node.value]: items }
    this.saveCurrentWorkspaceToCache(this.root)
    return items.map((entry) => this.toTreeNodeWithChildren(entry))
  }

  /** Selects and opens a file node from the sidebar tree by its workspace-relative path. */
  async select(path: string): Promise<void> {
    if (this.busy) return
    await this.openPath(path)
  }

  /** Opens `path` in an editor tab with an optional custom tab `title`, cursor `offset`, and editor `focus`. */
  async openPath(
    path: string,
    options?: { title?: string; offset?: number; focus?: boolean },
  ): Promise<void> {
    const entry = this.resolveFileEntry(path, options?.title)
    if (entry.kind === 'file') {
      await this.openFile(entry, options?.offset, options?.focus)
    }
  }

  /** Reads a file from the workspace, opens or activates its tab, and optionally positions the cursor at `targetOffset`. */
  async openFile(entry: WorkspaceNode, targetOffset?: number, focus = false): Promise<void> {
    const tabId = `file:${entry.path}`
    if (this.activeTabId === tabId && this.fileContent) {
      if (targetOffset !== undefined) {
        this.selectionOffset = null
        await Promise.resolve()
        this.selectionOffset = targetOffset
      }
      this.focusRequest++
      return
    }
    const requestId = ++this.openRequestSeq
    await this.flushSave()
    if (requestId !== this.openRequestSeq) return
    const existingTab = this.tabs.find((t) => t.id === tabId)
    this.selected = entry
    this.fileLoading = true
    this.openingError = null
    try {
      const content = await this.client.read(entry.path)
      if (this.isConfigFilePath(entry.path)) {
        void this.settings.load()
      }
      if (!this.active || requestId !== this.openRequestSeq) return
      this.fileContent = content
      this.lsp.recordFileStatus(content.lspStatus)
      this.upsertTab({
        id: tabId,
        kind: 'file',
        title: entry.name,
        path: entry.path,
        languageId: content.languageId ?? null,
        content: content.content,
        readonly: false,
        dirty: false,
        edited: Boolean(existingTab?.edited),
      })
      this.activeTabId = tabId
      this.selectionOffset = targetOffset ?? null
      this.fileVersion++
      if (focus || targetOffset !== undefined) {
        this.focusRequest++
      }
      this.saveCurrentWorkspaceToCache(this.root)
      void this.session.saveCurrentSession(this.root)
      void this.lsp.syncState()
    } catch (error) {
      if (this.active && requestId === this.openRequestSeq) {
        this.fileContent = null
        this.openingError = errorMessage(error)
      }
    } finally {
      if (this.active && requestId === this.openRequestSeq) this.fileLoading = false
    }
  }

  /** Activates an open file or diagnostics tab by its ID. */
  async activateTab(tabId: string): Promise<void> {
    if (this.blocked || this.activeTabId === tabId) return
    const tab = this.tabs.find((item) => item.id === tabId)
    if (!tab) return
    if (tab.kind === 'file' && tab.path) {
      await this.openFile(
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
    await this.flushSave()
    this.selected = null
    this.openingError = null
    this.activeTabId = tab.id
    this.selectionOffset = null
    this.fileContent = {
      content: tab.content,
      languageId: tab.languageId ?? null,
    }
    this.fileVersion++
    this.focusRequest++
    try {
      const refreshed = await this.lsp.getDiagnosticsBuffer(tab.languageId ?? undefined)
      if (!this.active) return
      this.tabs = this.tabs.map((item) =>
        item.id === tab.id ? { ...item, title: refreshed.title, content: refreshed.content } : item,
      )
      if (this.activeTabId === tab.id) {
        this.fileContent = {
          content: refreshed.content,
          languageId: refreshed.languageId ?? null,
        }
      }
    } catch {
      // Keep existing diagnostics buffer snapshot if refresh is unsupported.
    }
  }

  /** Flushes pending saves and closes the tab with `tabId`. */
  async closeTab(tabId: string): Promise<void> {
    if (this.blocked) return
    const index = this.tabs.findIndex((item) => item.id === tabId)
    if (index === -1) return
    await this.flushSave()
    const remaining = this.tabs.filter((item) => item.id !== tabId)
    this.tabs = remaining
    if (this.activeTabId !== tabId) {
      this.saveCurrentWorkspaceToCache(this.root)
      void this.session.saveCurrentSession(this.root)
      return
    }
    const fallback = remaining[index] ?? remaining[index - 1] ?? null
    if (!fallback) {
      this.clearSelected()
      this.saveCurrentWorkspaceToCache(this.root)
      void this.session.saveCurrentSession(this.root)
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
    if (this.blocked || this.tabs.length <= 1) return
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
    if (this.blocked || this.tabs.length <= 1) return
    const currentIdx = this.tabs.findIndex((tab) => tab.id === this.activeTabId)
    const startIdx = currentIdx === -1 ? 0 : currentIdx
    const step = Math.max(1, count)
    const nextIdx = (((startIdx - step) % this.tabs.length) + this.tabs.length) % this.tabs.length
    const target = this.tabs[nextIdx]
    if (target) {
      await this.activateTab(target.id)
    }
  }

  /** Reloads any open, clean `config.json` editor tabs from disk after a programmatic settings update. */
  async reloadCleanConfigTabs(): Promise<void> {
    for (const tab of this.tabs) {
      if (tab.kind !== 'file' || !tab.path || tab.dirty || !this.isConfigFilePath(tab.path)) {
        continue
      }
      if (this.dirtyPath === tab.path) continue
      try {
        const refreshed = await this.client.read(tab.path)
        if (!this.active) return
        this.tabs = this.tabs.map((item) =>
          item.id === tab.id && !item.dirty ? { ...item, content: refreshed.content } : item,
        )
        if (
          this.activeTabId === tab.id &&
          this.selected?.path === tab.path &&
          this.dirtyPath !== tab.path
        ) {
          this.fileContent = refreshed
        }
      } catch {
        // Ignore read errors if the config file does not exist yet.
      }
    }
  }

  /** Opens a read-only diagnostics buffer tab for all active servers or `languageId`. */
  async openDiagnosticsBuffer(languageId?: string): Promise<void> {
    await this.flushSave()
    try {
      const buffer = await this.lsp.getDiagnosticsBuffer(languageId)
      if (!this.active) return
      const tabId = languageId ? `diagnostics:${languageId}` : 'diagnostics:all'
      this.upsertTab({
        id: tabId,
        kind: 'diagnostics',
        title: buffer.title,
        languageId: buffer.languageId ?? null,
        content: buffer.content,
        readonly: true,
        dirty: false,
      })
      this.selected = null
      this.openingError = null
      this.activeTabId = tabId
      this.selectionOffset = null
      this.fileContent = {
        content: buffer.content,
        languageId: buffer.languageId ?? null,
      }
      this.fileVersion++
    } catch (error) {
      if (this.active) this.openingError = errorMessage(error)
    }
  }

  /** Opens `path` and moves the editor selection to `offset`. */
  async jumpToLocation(path: string, offset: number): Promise<void> {
    await this.openPath(path, { offset, focus: true })
  }

  /** Requests LSP hover documentation at `offset` in the currently selected file. */
  async requestHover(offset: number): Promise<CodeEditorHoverResult | null> {
    if (!this.selected) return null
    return this.lsp.requestHover(this.selected.path, offset)
  }

  /** Jumps to the first LSP definition location for the symbol at `offset`. */
  async gotoDefinition(offset: number): Promise<void> {
    if (!this.selected) return
    const locations = await this.lsp.getDefinitions(this.selected.path, offset)
    const first = locations[0]
    if (!first) return
    await this.jumpToLocation(first.path, first.offset)
  }

  /** Requests LSP completion items at `offset` in the currently selected file. */
  async requestCompletions(offset: number): Promise<CodeEditorCompletionItem[]> {
    if (!this.selected) return []
    return this.lsp.requestCompletions(this.selected.path, offset)
  }

  /** Requests LSP signature help at `offset` in the currently selected file. */
  async requestSignatureHelp(offset: number): Promise<CodeEditorSignatureHelp | null> {
    if (!this.selected) return null
    return this.lsp.requestSignatureHelp(this.selected.path, offset)
  }

  /** Updates the active editor buffer, marks the tab as `edited`, notifies LSP `didChange`, and schedules a 150ms debounced save. */
  updateContent(content: string): void {
    if (!this.selected || !this.fileContent || this.fileContent.content === content) return
    this.fileContent = { ...this.fileContent, content }
    const tabId = `file:${this.selected.path}`
    this.tabs = this.tabs.map((tab) =>
      tab.id === tabId ? { ...tab, content, dirty: true, edited: true } : tab,
    )
    this.dirtyPath = this.selected.path
    this.dirtyContent = content
    this.saveCurrentWorkspaceToCache(this.root)
    this.lsp.notifyDidChange(this.selected.path, content)
    if (this.saveTimer) clearTimeout(this.saveTimer)
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null
      void this.flushSave()
    }, 150)
  }

  /** Flushes any pending debounced file save to disk and updates persisted session state. */
  async flushSave(): Promise<void> {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer)
      this.saveTimer = null
    }
    if (this.inFlightSave) {
      await this.inFlightSave
    }
    while (this.dirtyPath && this.dirtyContent !== null) {
      const path = this.dirtyPath
      const content = this.dirtyContent
      this.dirtyPath = null
      this.dirtyContent = null
      this.saving = true
      const task = this.client
        .write(path, content)
        .then(async () => {
          if (!this.active) return
          const tabId = `file:${path}`
          this.tabs = this.tabs.map((tab) =>
            tab.id === tabId && tab.content === content
              ? { ...tab, dirty: false, edited: true }
              : tab,
          )
          this.saveCurrentWorkspaceToCache(this.root)
          void this.session.saveCurrentSession(this.root)
          if (this.isConfigFilePath(path)) {
            await this.settings.load()
          }
        })
        .catch((error) => {
          if (this.active) this.openingError = errorMessage(error)
        })
        .finally(() => {
          if (this.active) this.saving = false
          this.inFlightSave = null
        })
      this.inFlightSave = task
      await task
    }
  }

  /** Clears the active file selection and editor buffer. */
  clearSelected(): void {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer)
      this.saveTimer = null
    }
    this.dirtyPath = null
    this.dirtyContent = null
    this.selected = null
    this.fileContent = null
    this.openingError = null
    this.activeTabId = null
    this.selectionOffset = null
  }

  /** Creates a new untitled Markdown file inside `parent` and opens it in the editor. */
  async newFile(parent = ''): Promise<void> {
    if (this.cannotCreate) return
    await this.flushSave()
    this.busy = true
    this.creationError = null
    let created: WorkspaceNode | undefined
    try {
      const siblings = await this.client.list(parent)
      let number = 1
      let name = 'Untitled.md'
      while (siblings.some((entry) => entry.name === name)) name = `Untitled ${++number}.md`
      created = await this.client.createFile(name, parent)
      this.listed[created.path] = created
      if (!parent) this.entries = this.sortEntries([...this.entries, created])
      else this.refresh(parent)
    } catch (error) {
      this.creationError = errorMessage(error)
    } finally {
      this.busy = false
    }
    if (created) {
      await this.openFile(created)
      this.focusRequest++
    }
  }

  /** Begins creating a new file or folder inside `parent`. */
  startCreate(kind: 'file' | 'directory', parent = ''): void {
    if (kind === 'file') {
      void this.newFile(parent)
      return
    }
    if (this.cannotCreate) return
    this.pendingParent = parent
    let number = 1
    let name = 'New folder'
    while (this.entries.some((entry) => entry.name === name)) name = `New folder ${++number}`
    this.pendingEntry = { path: '__lgtm-new__', name, kind }
  }

  /** Commits an inline folder creation or entry rename in the workspace tree. */
  async commitEntry(path: string, name: string): Promise<void> {
    if (this.blocked) throw new Error('Wait for the current operation to finish.')
    await this.flushSave()
    this.busy = true
    let openNext: WorkspaceNode | undefined
    try {
      if (this.pendingEntry?.path === path) {
        const created = await this.client.createFolder(name, this.pendingParent)
        this.listed[created.path] = created
        if (!this.pendingParent) this.entries = this.sortEntries([...this.entries, created])
        else this.refresh(this.pendingParent)
        this.pendingEntry = null
      } else {
        const renamed = await this.client.rename(path, name)
        this.listed[renamed.path] = renamed
        this.tabs = this.tabs.map((tab) => {
          if (tab.kind !== 'file' || !tab.path) return tab
          if (tab.path === path) {
            return {
              ...tab,
              id: `file:${renamed.path}`,
              path: renamed.path,
              title: renamed.name,
            }
          }
          if (tab.path.startsWith(`${path}/`)) {
            const nextPath = renamed.path + tab.path.slice(path.length)
            return {
              ...tab,
              id: `file:${nextPath}`,
              path: nextPath,
            }
          }
          return tab
        })
        const selected = this.selected
        if (selected && (selected.path === path || selected.path.startsWith(`${path}/`))) {
          openNext = {
            ...selected,
            path: renamed.path + selected.path.slice(path.length),
            name: selected.path === path ? renamed.name : selected.name,
          }
          this.clearSelected()
        }
        try {
          this.entries = await this.client.list('')
          await this.preloadExpandedDirs(this.expandedDirs, true)
        } catch (error) {
          this.creationError = errorMessage(error)
        }
        this.treeVersion++
      }
    } finally {
      this.busy = false
    }
    if (openNext) await this.openFile(openNext)
  }

  /** Executes a context menu action (`new-file`, `new-folder`, or `delete`) on `path`. */
  async contextAction(path: string, action: string): Promise<void> {
    if (action === 'new-file' || action === 'new-folder') {
      this.startCreate(action === 'new-file' ? 'file' : 'directory', path)
      return
    }
    if (action !== 'delete' || this.blocked) return
    if (this.dirtyPath && (this.dirtyPath === path || this.dirtyPath.startsWith(`${path}/`))) {
      if (this.saveTimer) {
        clearTimeout(this.saveTimer)
        this.saveTimer = null
      }
      this.dirtyPath = null
      this.dirtyContent = null
    } else {
      await this.flushSave()
    }
    this.busy = true
    this.creationError = null
    try {
      await this.client.delete(path)
      this.tabs = this.tabs.filter(
        (tab) =>
          tab.kind !== 'file' ||
          !tab.path ||
          (tab.path !== path && !tab.path.startsWith(`${path}/`)),
      )
      const selected = this.selected
      if (selected && (selected.path === path || selected.path.startsWith(`${path}/`))) {
        this.clearSelected()
      }
      try {
        this.entries = await this.client.list('')
        await this.preloadExpandedDirs(this.expandedDirs, true)
      } catch (error) {
        this.creationError = errorMessage(error)
      }
      this.treeVersion++
    } catch (error) {
      this.creationError = errorMessage(error)
    } finally {
      this.busy = false
    }
  }

  /** Builds context menu items for a sidebar tree node. */
  contextItems(node: TreeNode): ContextMenuItem[] {
    return [
      ...(node.branch
        ? [
            { value: 'new-file', label: 'New file' },
            { value: 'new-folder', label: 'New folder' },
          ]
        : []),
      { value: 'rename', label: 'Rename', shortcut: 'F2' },
      { value: 'delete', label: 'Move to Trash', destructive: true },
    ]
  }

  /** Flushes any dirty buffer write and marks the service inactive on component teardown. */
  dispose(): void {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer)
      this.saveTimer = null
    }
    if (this.dirtyPath && this.dirtyContent !== null) {
      void this.client.write(this.dirtyPath, this.dirtyContent)
      this.dirtyPath = null
      this.dirtyContent = null
    }
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

  private isConfigFilePath(path: string): boolean {
    return (
      path === '~/.lgtm/config.json' ||
      path === '.lgtm/config.json' ||
      path === this.settings.snapshot?.globalPath ||
      path === this.settings.snapshot?.projectPath
    )
  }

  private buildSnapshotForCurrentRoot(): CachedWorkspaceSnapshot {
    const retainedTabs = this.tabs.filter(
      (tab) =>
        tab.kind === 'file' &&
        Boolean(tab.path) &&
        (Boolean(tab.edited) || tab.id === this.activeTabId),
    )
    const activeTab =
      retainedTabs.find((tab) => tab.id === this.activeTabId) ?? retainedTabs.at(-1) ?? null
    return {
      root: this.root,
      entries: [...this.entries],
      dirEntries: { ...this.dirEntries },
      listed: { ...this.listed },
      tabs: retainedTabs,
      activeTabId: activeTab?.id ?? null,
      selected: activeTab?.path
        ? { path: activeTab.path, name: activeTab.title, kind: 'file' }
        : null,
      fileContent:
        activeTab && this.fileContent
          ? { ...this.fileContent, content: activeTab.content }
          : activeTab
            ? {
                content: activeTab.content,
                languageId: activeTab.languageId ?? null,
              }
            : null,
      selectionOffset: this.selectionOffset,
      expandedDirs: [...this.expandedDirs],
    }
  }

  private saveCurrentWorkspaceToCache(root: string): void {
    if (!root || this.isDefaultPlaceholderRoot(root)) return
    this.workspaceCache.set(root, this.buildSnapshotForCurrentRoot())
  }

  private applyCachedWorkspace(cached: CachedWorkspaceSnapshot): void {
    this.loading = false
    this.error = null
    this.creationError = null
    this.openingError = null
    this.pendingEntry = null
    this.pendingParent = ''
    this.root = cached.root
    this.entries = cached.entries
    this.dirEntries = cached.dirEntries
    this.listed = cached.listed
    this.tabs = cached.tabs
    this.activeTabId = cached.activeTabId
    this.selected = cached.selected
    this.fileContent = cached.fileContent
    this.selectionOffset = cached.selectionOffset
    this.expandedDirs = cached.expandedDirs
    this.lsp.reset()
    this.treeVersion++
    this.fileVersion++
  }

  private toTreeNodeWithChildren(entry: WorkspaceNode): TreeNode {
    const node = new WorkspaceNodeViewModel(entry).toTreeNode()
    if (entry.kind === 'directory' && this.dirEntries[entry.path]) {
      return {
        ...node,
        children: this.dirEntries[entry.path]!.map((child) => this.toTreeNodeWithChildren(child)),
      }
    }
    return node
  }

  private async preloadExpandedDirs(dirs: string[], forceRefresh = false): Promise<void> {
    if (dirs.length === 0) return
    const targets = forceRefresh ? dirs : dirs.filter((dir) => !(dir in this.dirEntries))
    if (targets.length === 0) return
    const results = await Promise.all(
      targets.map(async (dir) => {
        try {
          const items = await this.client.list(dir)
          return { dir, items }
        } catch {
          return null
        }
      }),
    )
    if (!this.active) return
    const nextDirEntries = forceRefresh ? {} : { ...this.dirEntries }
    for (const res of results) {
      if (!res) continue
      nextDirEntries[res.dir] = res.items
      for (const item of res.items) {
        this.listed[item.path] = item
      }
    }
    this.dirEntries = nextDirEntries
  }

  private async refreshWorkspaceInBackground(path: string): Promise<void> {
    try {
      const result = await this.client.open(path)
      if (!this.active || this.root !== path) return
      this.root = result.root
      this.settings.rememberRoot(result.root)
      this.entries = result.entries
      void this.settings.load()
      void this.lsp.syncState()
      await this.session.syncSession(false)
      this.saveCurrentWorkspaceToCache(this.root)
    } catch (error) {
      if (this.active && this.root === path) {
        this.error = errorMessage(error)
      }
    }
  }

  private async restorePersistedTabs(
    persistedTabs: { path: string; title: string; edited: boolean; lastOpened: boolean }[],
  ): Promise<void> {
    const restored: WorkspaceTab[] = []
    let activeId: string | null = null
    let activeEntry: WorkspaceNode | null = null
    let activeContent: FileContent | null = null

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
      this.selected = activeEntry
      this.fileContent = activeContent
      this.fileVersion++
      this.saveCurrentWorkspaceToCache(this.root)
    }
  }

  private sortEntries(entries: WorkspaceNode[]): WorkspaceNode[] {
    return [...entries].sort(
      (a, b) =>
        Number(a.kind !== 'directory') - Number(b.kind !== 'directory') ||
        a.name.localeCompare(b.name),
    )
  }

  private refresh(path: string): void {
    this.refreshFolder = { path, version: (this.refreshFolder?.version ?? 0) + 1 }
  }

  private resolveFileEntry(path: string, title?: string): WorkspaceNode {
    if (title) {
      return { path, name: title, kind: 'file' }
    }
    return (
      this.entries.find((item) => item.path === path) ??
      this.listed[path] ?? {
        path,
        name: path.split('/').at(-1) ?? path,
        kind: 'file' as const,
      }
    )
  }

  private upsertTab(tab: WorkspaceTab): void {
    const existingIdx = this.tabs.findIndex((item) => item.id === tab.id)
    if (existingIdx === -1) {
      this.tabs = [...this.tabs, tab]
    } else {
      this.tabs = this.tabs.map((item, idx) => (idx === existingIdx ? tab : item))
    }
  }

  private focusTargetPane(paneId: 'projects' | 'workspace' | 'editor' | 'right-dock'): void {
    if (typeof document === 'undefined') return
    void tick().then(() => {
      requestAnimationFrame(() => {
        getDocumentWhichKeyController(document)?.focusPane(paneId)
      })
    })
  }
}
