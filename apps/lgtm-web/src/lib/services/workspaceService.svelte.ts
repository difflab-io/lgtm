import type { TreeNode, ContextMenuItem, CodeEditorHoverResult, TabItem } from 'lgtm-ui'
import { WorkspaceNodeViewModel } from '../models/workspaceNodeViewModel'
import {
  errorMessage,
  type WorkspaceClient,
  type WorkspaceNode,
  type FileContent,
  type ConfigFormat,
  type ConfigScope,
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
  kind: 'file' | 'diagnostics' | 'config'
  title: string
  path?: string
  lspPath?: string
  configScope?: ConfigScope
  configFormat?: ConfigFormat
  languageId?: string | null
  content: string
  readonly: boolean
  dirty?: boolean
}

/** Workspace view model. Rust retains filesystem ownership, path validation, and LSP orchestration. */
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
  private dirtyConfig: {
    scope: ConfigScope
    format?: ConfigFormat
    content: string
  } | null = null
  private saveTimer: ReturnType<typeof setTimeout> | null = null
  private inFlightSave: Promise<void> | null = null
  private openRequestSeq = 0

  constructor(private client: WorkspaceClient) {
    this.lsp = new LspService(client)
    this.settings = new SettingsService(client)
  }

  get blocked() {
    return this.busy || this.fileLoading
  }

  get cannotCreate() {
    return this.blocked || this.loading || Boolean(this.error || this.pendingEntry)
  }

  get activeTab(): WorkspaceTab | null {
    if (!this.activeTabId) return null
    return this.tabs.find((tab) => tab.id === this.activeTabId) ?? null
  }

  get tabItems(): TabItem[] {
    return this.tabs.map((tab) => ({
      value: tab.id,
      label: tab.title,
      dirty: Boolean(tab.dirty),
      closable: true,
    }))
  }

  get selectedTitle() {
    if (this.activeTab?.kind === 'diagnostics' || this.activeTab?.kind === 'config') {
      return this.activeTab.title
    }
    return this.selected ? this.selected.name : 'No file selected'
  }

  get selectedPath() {
    if (this.activeTab?.kind === 'diagnostics') return this.activeTab.title
    if (this.activeTab?.kind === 'config') return this.activeTab.path ?? this.activeTab.title
    return this.selected?.path
  }

  get activeEditorFilename(): string {
    if (this.activeTab?.kind === 'config') {
      return (
        this.activeTab.path?.split(/[\\/]/).at(-1) ??
        (this.activeTab.configFormat === 'yaml' ? 'config.yaml' : 'config.json')
      )
    }
    if (this.activeTab?.kind === 'diagnostics') return 'diagnostics.md'
    return this.selected?.name ?? 'diagnostics.md'
  }

  get breadcrumbs(): string[] {
    if (this.activeTab?.kind === 'diagnostics') return [this.activeTab.title]
    if (this.activeTab?.kind === 'config') {
      return ['.lgtm', this.activeEditorFilename]
    }
    if (!this.selected) return []
    return this.selected.path.split(/[\\/]/).filter(Boolean)
  }

  private get activeLspPath(): string | undefined {
    if (this.activeTab?.kind === 'config') {
      return this.activeTab.lspPath ?? this.activeTab.path
    }
    return this.selected?.path
  }

  get activeDiagnostics(): LspDiagnostic[] {
    return this.lsp.getDiagnosticsForPath(this.activeLspPath)
  }

  get activeLanguageId(): string | null {
    if (this.activeTab?.kind === 'diagnostics' || this.activeTab?.kind === 'config') {
      return this.activeTab.languageId ?? null
    }
    return this.fileContent?.languageId ?? null
  }

  get activeLspServer(): LspServerStatusEvent | null {
    return this.lsp.getActiveServer(this.activeLanguageId)
  }

  get allLspServers(): LspServerStatusEvent[] {
    return this.lsp.allServers
  }

  get lspDiagnostics(): Record<string, LspDiagnostic[]> {
    return this.lsp.diagnostics
  }

  get status() {
    if (this.openingError) return this.openingError
    if (this.fileLoading) return 'Opening file…'
    if (this.saving) return 'Saving…'
    if (this.selected || this.activeTab) return 'Ready'
    return 'Idle'
  }

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

  get nodes() {
    return [
      ...(this.pendingEntry && !this.pendingParent
        ? [new WorkspaceNodeViewModel(this.pendingEntry).toTreeNode()]
        : []),
      ...this.entries.map((entry) => new WorkspaceNodeViewModel(entry).toTreeNode()),
    ]
  }

  get pendingNode() {
    return this.pendingEntry
      ? new WorkspaceNodeViewModel(this.pendingEntry).toTreeNode()
      : undefined
  }

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
      if (path) {
        void this.settings.setWorkspaceRoots(result.root)
      } else {
        void this.settings.load()
      }
      void this.syncLspState()
    } catch (error) {
      if (this.active) this.error = errorMessage(error)
    } finally {
      if (this.active) this.loading = false
    }
  }

  async openSettingsTab(scope: ConfigScope = 'global', format?: ConfigFormat) {
    const requestId = ++this.openRequestSeq
    await this.flushSave()
    if (requestId !== this.openRequestSeq) return
    this.selected = null
    this.fileLoading = true
    this.openingError = null
    try {
      const doc = await this.settings.openDocument(scope, format ?? null)
      if (!doc || !this.active || requestId !== this.openRequestSeq) return
      const tabId = `config:${doc.scope}`
      this.upsertTab({
        id: tabId,
        kind: 'config',
        title: doc.title,
        path: doc.path,
        lspPath: doc.lspPath,
        configScope: doc.scope,
        configFormat: doc.format,
        languageId: doc.languageId,
        content: doc.content,
        readonly: false,
        dirty: false,
      })
      this.activeTabId = tabId
      this.selectionOffset = null
      this.fileContent = {
        content: doc.content,
        revision: doc.revision,
        languageId: doc.languageId,
      }
      this.fileVersion++
      this.focusRequest++
      void this.settings.load()
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

  async loadChildren(node: TreeNode) {
    if (node.value === this.pendingEntry?.path) return []
    const items = await this.client.list(node.value)
    if (!this.active) return []
    for (const entry of items) this.listed[entry.path] = entry
    return items.map((entry) => new WorkspaceNodeViewModel(entry).toTreeNode())
  }

  async select(path: string) {
    if (this.busy) return
    const entry = this.resolveFileEntry(path)
    if (entry.kind === 'file') await this.openFile(entry)
  }

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
    if (tab.kind === 'config' && tab.configScope) {
      await this.openSettingsTab(tab.configScope, tab.configFormat)
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

  async restartLanguageServer(languageId: string) {
    await this.lsp.restartServer(languageId)
  }

  async jumpToLocation(path: string, offset: number) {
    const entry = this.resolveFileEntry(path)
    await this.openFile(entry, offset)
    this.focusRequest++
  }

  async requestHover(offset: number): Promise<CodeEditorHoverResult | null> {
    const targetPath = this.activeLspPath
    if (!targetPath) return null
    return this.lsp.requestHover(targetPath, offset)
  }

  async gotoDefinition(offset: number): Promise<void> {
    const targetPath = this.activeLspPath
    if (!targetPath) return
    const locations = await this.lsp.getDefinitions(targetPath, offset)
    const first = locations[0]
    if (!first) return
    await this.jumpToLocation(first.path, first.offset)
  }

  handleLspStatus(event: LspServerStatusEvent) {
    if (!this.active) return
    this.lsp.handleStatus(event)
  }

  handleLspDiagnostics(event: LspFileDiagnosticsEvent) {
    if (!this.active) return
    this.lsp.handleDiagnostics(event)
    if (this.activeTab?.kind === 'diagnostics') {
      void this.openDiagnosticsBuffer(this.activeTab.languageId ?? undefined)
    }
  }

  async syncLspState() {
    if (!this.active) return
    await this.lsp.syncState()
  }

  updateContent(content: string) {
    if (!this.fileContent || this.fileContent.content === content) return
    if (this.activeTab?.kind === 'config' && this.activeTab.configScope) {
      const scope = this.activeTab.configScope
      const format = this.activeTab.configFormat
      const tabId = `config:${scope}`
      this.fileContent = { ...this.fileContent, content }
      this.tabs = this.tabs.map((tab) =>
        tab.id === tabId ? { ...tab, content, dirty: true } : tab,
      )
      this.dirtyConfig = { scope, format, content }
      const lspPath = this.activeLspPath
      if (lspPath) {
        this.lsp.notifyDidChange(lspPath, content)
      }
      if (this.saveTimer) clearTimeout(this.saveTimer)
      this.saveTimer = setTimeout(() => {
        this.saveTimer = null
        void this.flushSave()
      }, 150)
      return
    }
    if (!this.selected) return
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

  async flushSave() {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer)
      this.saveTimer = null
    }
    if (this.inFlightSave) {
      await this.inFlightSave
    }
    while (this.dirtyConfig) {
      const { scope, format, content } = this.dirtyConfig
      this.dirtyConfig = null
      this.saving = true
      const task = this.settings
        .writeDocument(scope, content, format ?? null)
        .then(async (savedDoc) => {
          if (!this.active || !savedDoc) return
          const tabId = `config:${scope}`
          this.tabs = this.tabs.map((tab) =>
            tab.id === tabId && tab.content === content
              ? { ...tab, dirty: false, lspPath: savedDoc.lspPath }
              : tab,
          )
          if (this.activeTabId === tabId && this.fileContent?.content === content) {
            this.fileContent = {
              ...this.fileContent,
              revision: savedDoc.revision,
              languageId: savedDoc.languageId,
            }
          }
          await this.settings.load()
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
    while (this.dirtyPath && this.dirtyContent !== null) {
      const path = this.dirtyPath
      const content = this.dirtyContent
      this.dirtyPath = null
      this.dirtyContent = null
      this.saving = true
      const task = this.client
        .write(path, content)
        .then((saved) => {
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

  clearSelected() {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer)
      this.saveTimer = null
    }
    this.dirtyPath = null
    this.dirtyContent = null
    this.dirtyConfig = null
    this.selected = null
    this.fileContent = null
    this.openingError = null
    this.activeTabId = null
    this.selectionOffset = null
  }

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

  dispose() {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer)
      this.saveTimer = null
    }
    if (this.dirtyConfig) {
      void this.settings.writeDocument(
        this.dirtyConfig.scope,
        this.dirtyConfig.content,
        this.dirtyConfig.format ?? null,
      )
      this.dirtyConfig = null
    }
    if (this.dirtyPath && this.dirtyContent !== null) {
      void this.client.write(this.dirtyPath, this.dirtyContent)
      this.dirtyPath = null
      this.dirtyContent = null
    }
    this.active = false
  }

  async refreshCleanConfigTabs() {
    if (!this.active || this.dirtyConfig) return
    for (const tab of this.tabs) {
      if (tab.kind !== 'config' || !tab.configScope || tab.dirty) continue
      try {
        const doc = await this.settings.openDocument(tab.configScope, tab.configFormat ?? null)
        if (!doc || !this.active) return
        this.tabs = this.tabs.map((item) =>
          item.id === tab.id && !item.dirty
            ? {
                ...item,
                title: doc.title,
                path: doc.path,
                lspPath: doc.lspPath,
                content: doc.content,
              }
            : item,
        )
        if (this.activeTabId === tab.id && this.fileContent && !tab.dirty) {
          this.fileContent = {
            content: doc.content,
            revision: doc.revision,
            languageId: doc.languageId,
          }
        }
      } catch {
        // Ignore refresh failures if config document cannot be read.
      }
    }
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
