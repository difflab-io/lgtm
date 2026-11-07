import type { TreeNode, ContextMenuItem, CodeEditorHoverResult, TabItem } from 'lgtm-ui'
import { WorkspaceNodeViewModel } from '../models/workspaceNodeViewModel'
import {
  errorMessage,
  type WorkspaceClient,
  type WorkspaceNode,
  type FileContent,
  type LspDiagnostic,
  type LspFileDiagnosticsEvent,
  type LspServerStatusEvent,
} from '../clients/workspaceClient'
import { LspService } from './lspService.svelte'
import { SettingsService } from './settingsService.svelte'

export interface FolderRefresh {
  path: string
  version: number
}

export interface WorkspaceTab {
  id: string
  kind: 'file' | 'diagnostics'
  title: string
  path?: string
  languageId?: string | null
  content: string
  readonly: boolean
  dirty?: boolean
}

/** Workspace view model coordinating file tree browsing, open tabs, debounced saves, LSP state, and settings. */
export class WorkspaceService {
  readonly lsp: LspService
  readonly settings: SettingsService
  defaultRoot = $state('')
  root = $state('~/.lgtm/workspace')
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
  tabs = $state<WorkspaceTab[]>([])
  activeTabId = $state<string | null>(null)
  selectionOffset = $state<number | null>(null)
  private listed = $state<Record<string, WorkspaceNode>>({})
  private active = true
  private dirtyPath: string | null = null
  private dirtyContent: string | null = null
  private saveTimer: ReturnType<typeof setTimeout> | null = null
  private inFlightSave: Promise<void> | null = null
  private openRequestSeq = 0

  /** Constructs a `WorkspaceService` with its own `LspService` and `SettingsService` instances. */
  constructor(private client: WorkspaceClient) {
    this.lsp = new LspService(client)
    this.settings = new SettingsService(client)
  }

  /** Returns whether a file or tree operation is currently in progress. */
  get blocked() {
    return this.busy || this.fileLoading
  }

  /** Returns whether creating a new file or folder is currently disabled. */
  get cannotCreate() {
    return this.blocked || this.loading || Boolean(this.error || this.pendingEntry)
  }

  /** Returns the currently active editor tab, if any. */
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
  get selectedTitle() {
    if (this.activeTab?.kind === 'diagnostics') {
      return this.activeTab.title
    }
    return this.selected ? this.selected.name : 'No file selected'
  }

  /** Returns the path or title shown in the footer breadcrumb tooltip. */
  get selectedPath() {
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

  /** Returns LSP diagnostics for the currently selected file. */
  get activeDiagnostics(): LspDiagnostic[] {
    return this.lsp.getDiagnosticsForPath(this.selected?.path)
  }

  /** Returns the language ID of the active editor buffer. */
  get activeLanguageId(): string | null {
    if (this.activeTab?.kind === 'diagnostics') {
      return this.activeTab.languageId ?? null
    }
    return this.fileContent?.languageId ?? null
  }

  /** Returns the active language server status for the current file or workspace. */
  get activeLspServer(): LspServerStatusEvent | null {
    return this.lsp.getActiveServer(this.activeLanguageId)
  }

  /** Returns all language servers tracked in the active workspace. */
  get allLspServers(): LspServerStatusEvent[] {
    return this.lsp.allServers
  }

  /** Returns the map of workspace file paths to LSP diagnostics. */
  get lspDiagnostics(): Record<string, LspDiagnostic[]> {
    return this.lsp.diagnostics
  }

  /** Returns the human-readable editor status label for the footer. */
  get status() {
    if (this.openingError) return this.openingError
    if (this.fileLoading) return 'Opening file…'
    if (this.saving) return 'Saving…'
    if (this.selected || this.activeTab) return 'Ready'
    return 'Idle'
  }

  /** Returns selectable workspace items for the titlebar workspace switcher. */
  get items() {
    const roots = this.settings.recentRoots.length ? this.settings.recentRoots : [this.root]
    return [
      ...roots.map((path) => ({
        label:
          path === this.defaultRoot || this.isDefaultWorkspaceRoot(path)
            ? 'Local workspace'
            : (path.split(/[\\/]/).filter(Boolean).at(-1) ?? path),
        value: path,
      })),
      { label: 'Open folder…', value: '__open_folder__' },
    ]
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

  /** Returns top-level tree nodes for the sidebar `TreeView`. */
  get nodes() {
    return [
      ...(this.pendingEntry && !this.pendingParent
        ? [new WorkspaceNodeViewModel(this.pendingEntry).toTreeNode()]
        : []),
      ...this.entries.map((entry) => new WorkspaceNodeViewModel(entry).toTreeNode()),
    ]
  }

  /** Returns the pending inline folder creation node, if active. */
  get pendingNode() {
    return this.pendingEntry
      ? new WorkspaceNodeViewModel(this.pendingEntry).toTreeNode()
      : undefined
  }

  /** Opens the default or specified workspace directory and refreshes settings and LSP state. */
  async open(path?: string) {
    await this.flushSave()
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
      this.root = result.root
      this.settings.rememberRoot(result.root)
      this.entries = result.entries
      this.listed = {}
      this.tabs = []
      this.activeTabId = null
      this.lsp.reset()
      this.clearSelected()
      this.treeVersion++
      void this.settings.load()
      void this.syncLspState()
    } catch (error) {
      if (this.active) this.error = errorMessage(error)
    } finally {
      if (this.active) this.loading = false
    }
  }

  /** Ensures `~/.lgtm/config.json` exists and opens it in `CodeEditor` as a standard file tab. */
  async openGlobalConfig() {
    if (!this.client.openGlobalConfig) return
    const path = '~/.lgtm/config.json'
    const tabId = `file:${path}`
    if (this.activeTabId === tabId && this.fileContent) {
      this.focusRequest++
      return
    }
    const requestId = ++this.openRequestSeq
    await this.flushSave()
    if (requestId !== this.openRequestSeq) return
    this.fileLoading = true
    this.openingError = null
    try {
      const content = await this.client.openGlobalConfig()
      void this.settings.load()
      if (!this.active || requestId !== this.openRequestSeq) return
      const title = 'Settings'
      this.selected = { path, name: title, kind: 'file' }
      this.fileContent = content
      this.lsp.recordFileStatus(content.lspStatus)
      this.upsertTab({
        id: tabId,
        kind: 'file',
        title,
        path,
        languageId: content.languageId ?? 'json',
        content: content.content,
        readonly: false,
        dirty: false,
      })
      this.activeTabId = tabId
      this.selectionOffset = null
      this.fileVersion++
      this.focusRequest++
      void this.syncLspState()
    } catch (error) {
      if (this.active && requestId === this.openRequestSeq) {
        this.fileContent = null
        this.openingError = errorMessage(error)
      }
    } finally {
      if (this.active && requestId === this.openRequestSeq) this.fileLoading = false
    }
  }

  /** Ensures `<workspace>/.lgtm/config.json` exists and opens it in `CodeEditor` as a standard file tab. */
  async openLocalConfig() {
    if (!this.client.openLocalConfig) return
    const path = '.lgtm/config.json'
    const tabId = `file:${path}`
    if (this.activeTabId === tabId && this.fileContent) {
      this.focusRequest++
      return
    }
    const requestId = ++this.openRequestSeq
    await this.flushSave()
    if (requestId !== this.openRequestSeq) return
    this.fileLoading = true
    this.openingError = null
    try {
      const content = await this.client.openLocalConfig()
      void this.settings.load()
      if (!this.active || requestId !== this.openRequestSeq) return
      const title = 'Settings (project)'
      this.selected = { path, name: title, kind: 'file' }
      this.fileContent = content
      this.lsp.recordFileStatus(content.lspStatus)
      this.upsertTab({
        id: tabId,
        kind: 'file',
        title,
        path,
        languageId: content.languageId ?? 'json',
        content: content.content,
        readonly: false,
        dirty: false,
      })
      this.activeTabId = tabId
      this.selectionOffset = null
      this.fileVersion++
      this.focusRequest++
      void this.syncLspState()
    } catch (error) {
      if (this.active && requestId === this.openRequestSeq) {
        this.fileContent = null
        this.openingError = errorMessage(error)
      }
    } finally {
      if (this.active && requestId === this.openRequestSeq) this.fileLoading = false
    }
  }

  /** Switches to a selected workspace root or opens the native folder picker dialog. */
  async selectWorkspace(value: string) {
    if (this.blocked) return
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
  async loadChildren(node: TreeNode) {
    if (node.value === this.pendingEntry?.path) return []
    const items = await this.client.list(node.value)
    if (!this.active) return []
    for (const entry of items) this.listed[entry.path] = entry
    return items.map((entry) => new WorkspaceNodeViewModel(entry).toTreeNode())
  }

  /** Selects and opens a file node from the sidebar tree by its workspace-relative path. */
  async select(path: string) {
    if (this.busy) return
    const entry = this.resolveFileEntry(path)
    if (entry.kind === 'file') await this.openFile(entry)
  }

  /** Reads a file from the workspace, opens or activates its tab, and optionally positions the cursor at `targetOffset`. */
  async openFile(entry: WorkspaceNode, targetOffset?: number) {
    const requestId = ++this.openRequestSeq
    await this.flushSave()
    if (requestId !== this.openRequestSeq) return
    this.selected = entry
    this.fileLoading = true
    this.openingError = null
    try {
      const content = await this.client.read(entry.path)
      if (!this.active || requestId !== this.openRequestSeq) return
      this.fileContent = content
      this.lsp.recordFileStatus(content.lspStatus)
      const tabId = `file:${entry.path}`
      this.upsertTab({
        id: tabId,
        kind: 'file',
        title: entry.name,
        path: entry.path,
        languageId: content.languageId ?? null,
        content: content.content,
        readonly: false,
        dirty: false,
      })
      this.activeTabId = tabId
      this.selectionOffset = targetOffset ?? null
      this.fileVersion++
      void this.syncLspState()
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
  async activateTab(tabId: string) {
    if (this.blocked || this.activeTabId === tabId) return
    const tab = this.tabs.find((item) => item.id === tabId)
    if (!tab) return
    if (tab.kind === 'file' && tab.path) {
      await this.openFile({
        path: tab.path,
        name: tab.title,
        kind: 'file',
      })
      return
    }
    await this.flushSave()
    this.selected = null
    this.openingError = null
    this.activeTabId = tab.id
    this.selectionOffset = null
    this.fileContent = {
      content: tab.content,
      revision: 'diagnostics',
      languageId: tab.languageId ?? null,
    }
    this.fileVersion++
    try {
      const refreshed = await this.lsp.getDiagnosticsBuffer(tab.languageId ?? undefined)
      if (!this.active) return
      this.tabs = this.tabs.map((item) =>
        item.id === tab.id ? { ...item, title: refreshed.title, content: refreshed.content } : item,
      )
      if (this.activeTabId === tab.id) {
        this.fileContent = {
          content: refreshed.content,
          revision: 'diagnostics',
          languageId: refreshed.languageId ?? null,
        }
      }
    } catch {
      // Keep existing diagnostics buffer snapshot if refresh is unsupported.
    }
  }

  /** Flushes pending saves and closes the tab with `tabId`. */
  async closeTab(tabId: string) {
    if (this.blocked) return
    const index = this.tabs.findIndex((item) => item.id === tabId)
    if (index === -1) return
    await this.flushSave()
    const remaining = this.tabs.filter((item) => item.id !== tabId)
    this.tabs = remaining
    if (this.activeTabId !== tabId) return
    const fallback = remaining[index] ?? remaining[index - 1] ?? null
    if (!fallback) {
      this.clearSelected()
      return
    }
    this.activeTabId = null
    await this.activateTab(fallback.id)
  }

  /** Opens a read-only diagnostics buffer tab for all active servers or `languageId`. */
  async openDiagnosticsBuffer(languageId?: string) {
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
        revision: 'diagnostics',
        languageId: buffer.languageId ?? null,
      }
      this.fileVersion++
    } catch (error) {
      if (this.active) this.openingError = errorMessage(error)
    }
  }

  /** Restarts the language server for `languageId`. */
  async restartLanguageServer(languageId: string) {
    await this.lsp.restartServer(languageId)
  }

  /** Opens `path` and moves the editor selection to `offset`. */
  async jumpToLocation(path: string, offset: number) {
    const entry = this.resolveFileEntry(path)
    await this.openFile(entry, offset)
    this.focusRequest++
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

  /** Handles an incoming `lsp-status` event from the desktop backend. */
  handleLspStatus(event: LspServerStatusEvent) {
    if (!this.active) return
    this.lsp.handleStatus(event)
  }

  /** Handles an incoming `lsp-diagnostics` event and refreshes any open diagnostics tab. */
  handleLspDiagnostics(event: LspFileDiagnosticsEvent) {
    if (!this.active) return
    this.lsp.handleDiagnostics(event)
    if (this.activeTab?.kind === 'diagnostics') {
      void this.openDiagnosticsBuffer(this.activeTab.languageId ?? undefined)
    }
  }

  /** Synchronizes workspace LSP state with the backend. */
  async syncLspState() {
    if (!this.active) return
    await this.lsp.syncState()
  }

  /** Updates the active editor buffer, notifies LSP `didChange`, and schedules a 150ms debounced save. */
  updateContent(content: string) {
    if (!this.selected || !this.fileContent || this.fileContent.content === content) return
    this.fileContent = { ...this.fileContent, content }
    const tabId = `file:${this.selected.path}`
    this.tabs = this.tabs.map((tab) => (tab.id === tabId ? { ...tab, content, dirty: true } : tab))
    this.dirtyPath = this.selected.path
    this.dirtyContent = content
    this.lsp.notifyDidChange(this.selected.path, content)
    if (this.saveTimer) clearTimeout(this.saveTimer)
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null
      void this.flushSave()
    }, 150)
  }

  /** Flushes any pending debounced file save to disk. */
  async flushSave() {
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
        .then(async (saved) => {
          if (!this.active) return
          const tabId = `file:${path}`
          this.tabs = this.tabs.map((tab) =>
            tab.id === tabId && tab.content === content ? { ...tab, dirty: false } : tab,
          )
          if (this.selected?.path === path && this.fileContent?.content === content) {
            this.fileContent = {
              ...this.fileContent,
              ...saved,
              languageId: saved.languageId ?? this.fileContent.languageId,
              lspStatus: saved.lspStatus ?? this.fileContent.lspStatus,
            }
          }
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
  clearSelected() {
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
  async newFile(parent = '') {
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
  startCreate(kind: 'file' | 'directory', parent = '') {
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
  async commitEntry(path: string, name: string) {
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
  async contextAction(path: string, action: string) {
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

  /** Flushes any dirty buffer and marks the service inactive on component teardown. */
  dispose() {
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

  private sortEntries(entries: WorkspaceNode[]) {
    return [...entries].sort(
      (a, b) =>
        Number(a.kind !== 'directory') - Number(b.kind !== 'directory') ||
        a.name.localeCompare(b.name),
    )
  }

  private refresh(path: string) {
    this.refreshFolder = { path, version: (this.refreshFolder?.version ?? 0) + 1 }
  }

  private resolveFileEntry(path: string): WorkspaceNode {
    return (
      this.entries.find((item) => item.path === path) ??
      this.listed[path] ?? {
        path,
        name: path.split('/').at(-1) ?? path,
        kind: 'file' as const,
      }
    )
  }

  private upsertTab(tab: WorkspaceTab) {
    const existingIdx = this.tabs.findIndex((item) => item.id === tab.id)
    if (existingIdx === -1) {
      this.tabs = [...this.tabs, tab]
    } else {
      this.tabs = this.tabs.map((item, idx) => (idx === existingIdx ? tab : item))
    }
  }
}
