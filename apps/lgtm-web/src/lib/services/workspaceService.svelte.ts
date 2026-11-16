import type {
  TreeNode,
  ContextMenuItem,
  CodeEditorCompletionItem,
  CodeEditorHoverResult,
  CodeEditorSignatureHelp,
} from 'lgtm-ui'
import { WorkspaceNodeViewModel } from '../models/workspaceNodeViewModel'
import {
  errorMessage,
  type WorkspaceClient,
  type WorkspaceNode,
  type FileContent,
} from '../clients/workspaceClient'
import type { CachedWorkspaceSnapshot, FolderRefresh, WorkspaceTab } from '../types'
import type { LspService } from './lspService.svelte'
import type { SessionService } from './sessionService.svelte'
import type { SettingsService } from './settingsService.svelte'

// TODO(DIFF-307): Rename `WorkspaceService` to `FileExplorerService` once the remaining active-buffer/file-explorer split finishes, and merge `libs/lgtm-workspace` into `apps/lgtm-desktop`.
/** File explorer tree and active editor buffer service backed by `lgtm-workspace` IPC commands. */
export class WorkspaceService {
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
  private active = true
  private dirtyPath: string | null = null
  private dirtyContent: string | null = null
  private saveTimer: ReturnType<typeof setTimeout> | null = null
  private inFlightSave: Promise<void> | null = null
  private openRequestSeq = 0

  /** Constructs a `WorkspaceService` scoped to file explorer tree state and file buffer I/O. */
  constructor(
    readonly client: WorkspaceClient,
    private readonly session: SessionService,
    private readonly lsp: LspService,
    private readonly settings: SettingsService,
  ) {}

  // Public --------------------------------------------------------------------

  /** Returns whether a file or tree operation is currently in progress. */
  get blocked(): boolean {
    return this.busy || this.fileLoading
  }

  /** Returns whether creating a new file or folder is currently disabled. */
  get cannotCreate(): boolean {
    return this.blocked || this.loading || Boolean(this.error || this.pendingEntry)
  }

  /** Returns the display title of the active tab or selected file. */
  get selectedTitle(): string {
    if (this.session.activeTab?.kind === 'diagnostics') {
      return this.session.activeTab.title
    }
    return this.selected ? this.selected.name : 'No file selected'
  }

  /** Returns the path or title shown in the footer breadcrumb tooltip. */
  get selectedPath(): string | undefined {
    if (this.session.activeTab?.kind === 'diagnostics') return this.session.activeTab.title
    return this.selected?.path
  }

  /** Returns the filename passed to `CodeEditor` for syntax highlighting. */
  get activeEditorFilename(): string {
    if (this.session.activeTab?.kind === 'diagnostics') return 'diagnostics.md'
    if (this.selected?.path.endsWith('config.json')) return 'config.json'
    return this.selected?.name ?? 'diagnostics.md'
  }

  /** Returns path segments rendered in the footer breadcrumb bar. */
  get breadcrumbs(): string[] {
    if (this.session.activeTab?.kind === 'diagnostics') return [this.session.activeTab.title]
    if (!this.selected) return []
    if (this.isConfigFilePath(this.selected.path)) {
      return ['.lgtm', 'config.json']
    }
    return this.selected.path.split(/[\\/]/).filter(Boolean)
  }

  /** Returns the language ID of the active editor buffer. */
  get activeLanguageId(): string | null {
    if (this.session.activeTab?.kind === 'diagnostics') {
      return this.session.activeTab.languageId ?? null
    }
    return this.fileContent?.languageId ?? null
  }

  /** Returns the human-readable editor status label for the footer. */
  get status(): string {
    if (this.openingError) return this.openingError
    if (this.fileLoading) return 'Opening file…'
    if (this.saving) return 'Saving…'
    if (this.selected || this.session.activeTab) return 'Ready'
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

  /** Returns the monotonic open-file sequence number for race detection during startup open. */
  getOpenRequestSeq(): number {
    return this.openRequestSeq
  }

  /** Clears transient explorer errors and marks loading active before switching workspaces. */
  beginWorkspaceOpen(): void {
    this.loading = true
    this.error = null
    this.creationError = null
    this.pendingEntry = null
    this.pendingParent = ''
  }

  /** Replaces the top-level directory entries for the active workspace root. */
  setEntries(entries: WorkspaceNode[]): void {
    this.entries = entries
  }

  /** Sets hydrated `expandedDirs` restored from persisted session state. */
  setHydratedExpandedDirs(dirs: string[]): void {
    this.expandedDirs = dirs
  }

  /** Applies an opened directory listing from `workspace_open` to the explorer tree and resets selection unless preserving startup selection. */
  applyOpenedListing(entries: WorkspaceNode[], preserveSelection: boolean): void {
    this.entries = entries
    if (!preserveSelection) {
      this.dirEntries = {}
      this.listed = {}
      this.expandedDirs = []
      this.clearSelected()
    }
  }

  /** Applies a cached tree and buffer snapshot during an instant workspace switch. */
  applyCachedWorkspace(cached: CachedWorkspaceSnapshot, entriesOverride?: WorkspaceNode[]): void {
    this.loading = false
    this.error = null
    this.creationError = null
    this.openingError = null
    this.pendingEntry = null
    this.pendingParent = ''
    this.entries = entriesOverride ?? cached.entries
    this.dirEntries = cached.dirEntries
    this.listed = cached.listed
    this.selected = cached.selected
    this.fileContent = cached.fileContent
    this.selectionOffset = cached.selectionOffset
    this.expandedDirs = cached.expandedDirs
  }

  /** Applies restored file selection and buffer content loaded from persisted session tabs. */
  applyRestoredBuffer(entry: WorkspaceNode | null, content: FileContent | null): void {
    this.selected = entry
    this.fileContent = content
    this.fileVersion++
  }

  /** Increments `treeVersion` to reset transient `TreeView` internal expansion cache cleanly. */
  bumpTreeVersion(): void {
    this.treeVersion++
  }

  /** Increments `fileVersion` to remount `CodeEditor` on buffer transitions. */
  bumpFileVersion(): void {
    this.fileVersion++
  }

  /** Builds a `CachedWorkspaceSnapshot` combining current explorer state with session `retainedTabs`. */
  buildSnapshot(
    root: string,
    retainedTabs: WorkspaceTab[],
    activeTab: WorkspaceTab | null,
  ): CachedWorkspaceSnapshot {
    return {
      root,
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

  /** Preloads directory listings for all expanded branches so the explorer renders synchronously in one frame. */
  async preloadExpandedDirs(dirs: string[], forceRefresh = false): Promise<void> {
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

  /** Updates the tracked list of expanded directory paths in `FileExplorer` and persists session state. */
  setExpandedDirs(dirs: string[]): void {
    this.expandedDirs = [...dirs]
    void this.session.saveCurrentWorkspaceSession()
  }

  /** Lazily loads child nodes for an expanded directory in the sidebar tree. */
  async loadChildren(node: TreeNode): Promise<TreeNode[]> {
    if (node.value === this.pendingEntry?.path) return []
    const items = await this.client.list(node.value)
    if (!this.active) return []
    for (const entry of items) this.listed[entry.path] = entry
    this.dirEntries = { ...this.dirEntries, [node.value]: items }
    this.session.saveCurrentWorkspaceToCache()
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
    if (this.session.activeTabId === tabId && this.fileContent) {
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
    const existingTab = this.session.tabs.find((t) => t.id === tabId)
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
      this.session.upsertAndActivateTab({
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
      this.selectionOffset = targetOffset ?? null
      this.fileVersion++
      if (focus || targetOffset !== undefined) {
        this.focusRequest++
      }
      this.session.saveCurrentWorkspaceToCache()
      void this.session.saveCurrentWorkspaceSession()
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

  /** Displays a cached diagnostics tab buffer and focuses the editor pane. */
  showDiagnosticsTabSnapshot(content: string, languageId: string | null): void {
    this.selected = null
    this.openingError = null
    this.selectionOffset = null
    this.fileContent = { content, languageId }
    this.fileVersion++
    this.focusRequest++
  }

  /** Updates the displayed diagnostics buffer content after a background refresh. */
  updateDiagnosticsContent(content: string, languageId: string | null): void {
    this.fileContent = { content, languageId }
  }

  /** Reloads any open, clean `config.json` editor tabs from disk after a programmatic settings update. */
  async reloadCleanConfigTabs(): Promise<void> {
    for (const tab of this.session.tabs) {
      if (tab.kind !== 'file' || !tab.path || tab.dirty || !this.isConfigFilePath(tab.path)) {
        continue
      }
      if (this.dirtyPath === tab.path) continue
      try {
        const refreshed = await this.client.read(tab.path)
        if (!this.active) return
        this.session.updateCleanTabContent(tab.id, refreshed.content)
        if (
          this.session.activeTabId === tab.id &&
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
      this.session.upsertAndActivateTab({
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
    this.session.markTabDirty(this.selected.path, content)
    this.dirtyPath = this.selected.path
    this.dirtyContent = content
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
          this.session.markTabSaved(path, content)
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
        this.session.handlePathRenamed(path, renamed)
        const selected = this.selected
        if (selected && (selected.path === path || selected.path.startsWith(`${path}/`))) {
          openNext = {
            ...selected,
            path: renamed.path + selected.path.slice(path.length),
            name: selected.path === path ? renamed.name : selected.name,
          }
          this.clearSelected()
          this.session.activeTabId = null
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
      this.session.handlePathsRemoved(path)
      const selected = this.selected
      if (selected && (selected.path === path || selected.path.startsWith(`${path}/`))) {
        this.clearSelected()
        this.session.activeTabId = null
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

  private isConfigFilePath(path: string): boolean {
    return (
      path === '~/.lgtm/config.json' ||
      path === '.lgtm/config.json' ||
      path === this.settings.snapshot?.globalPath ||
      path === this.settings.snapshot?.projectPath
    )
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
}
