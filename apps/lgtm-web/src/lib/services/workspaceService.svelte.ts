import type { TreeNode, ContextMenuItem } from 'lgtm-ui'
import { WorkspaceNodeViewModel } from '../models/workspaceNodeViewModel'
import {
  errorMessage,
  type WorkspaceClient,
  type WorkspaceNode,
  type FileContent,
} from '../clients/workspaceClient'

export interface FolderRefresh {
  path: string
  version: number
}

/** Workspace view model. Rust retains filesystem ownership and path validation. */
export class WorkspaceService {
  defaultRoot = $state('')
  root = $state('~/.lgtm/workspace')
  recentWorkspaces = $state<string[]>([])
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
  openingError = $state<string | null>(null)
  fileVersion = $state(0)
  focusRequest = $state(0)
  private listed = $state<Record<string, WorkspaceNode>>({})
  private active = true

  constructor(private client: WorkspaceClient) {}

  get blocked() {
    return this.busy || this.fileLoading
  }

  get cannotCreate() {
    return this.blocked || this.loading || Boolean(this.error || this.pendingEntry)
  }

  get selectedTitle() {
    return this.selected ? this.selected.name : 'No file selected'
  }

  get status() {
    if (this.openingError) return this.openingError
    if (this.fileLoading) return 'Opening file…'
    if (this.selected) return 'Ready'
    return 'Idle'
  }

  get items() {
    const roots = this.recentWorkspaces.length ? this.recentWorkspaces : [this.root]
    return [
      ...roots.map((path) => ({
        label:
          path === this.defaultRoot || path === '~/.lgtm/workspace'
            ? 'Local workspace'
            : (path.split(/[\\/]/).filter(Boolean).at(-1) ?? path),
        value: path,
      })),
      { label: 'Open folder…', value: '__open_folder__' },
    ]
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
    this.loading = true
    this.error = null
    this.creationError = null
    this.pendingEntry = null
    this.pendingParent = ''
    try {
      const result = await this.client.open(path)
      if (!this.active) return
      if (!this.defaultRoot && !path) {
        this.defaultRoot = result.root
      }
      this.root = result.root
      if (!this.recentWorkspaces.includes(result.root)) {
        this.recentWorkspaces = [...this.recentWorkspaces, result.root]
      }
      this.entries = result.entries
      this.listed = {}
      this.clearSelected()
      this.treeVersion++
    } catch (error) {
      if (this.active) this.error = errorMessage(error)
    } finally {
      if (this.active) this.loading = false
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
    if (this.blocked) return
    const entry = this.entries.find((item) => item.path === path) ?? this.listed[path]
    if (entry) await this.openFile(entry)
  }

  async openFile(entry: WorkspaceNode) {
    this.selected = entry
    this.fileLoading = true
    this.openingError = null
    try {
      const content = await this.client.read(entry.path)
      if (!this.active) return
      this.fileContent = content
      this.fileVersion++
    } catch (error) {
      if (this.active) {
        this.fileContent = null
        this.openingError = errorMessage(error)
      }
    } finally {
      if (this.active) this.fileLoading = false
    }
  }

  clearSelected() {
    this.selected = null
    this.fileContent = null
    this.openingError = null
  }

  async newFile(parent = '') {
    if (this.cannotCreate) return
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
      if (!parent) this.entries = [...this.entries, created]
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
    this.busy = true
    let openNext: WorkspaceNode | undefined
    try {
      if (this.pendingEntry?.path === path) {
        const created =
          this.pendingEntry.kind === 'directory'
            ? await this.client.createFolder(name, this.pendingParent)
            : await this.client.createFile(name, this.pendingParent)
        this.listed[created.path] = created
        if (!this.pendingParent)
          this.entries = [...this.entries, created].sort(
            (a, b) =>
              Number(a.kind !== 'directory') - Number(b.kind !== 'directory') ||
              a.name.localeCompare(b.name),
          )
        else this.refresh(this.pendingParent)
        this.pendingEntry = null
        if (created.kind === 'file') openNext = created
      } else {
        const renamed = await this.client.rename(path, name)
        this.listed[renamed.path] = renamed
        const selected = this.selected
        if (selected && (selected.path === path || selected.path.startsWith(`${path}/`))) {
          openNext = {
            ...selected,
            path: renamed.path + selected.path.slice(path.length),
            name: selected.path === path ? renamed.name : selected.name,
          }
          this.clearSelected()
        }
        this.entries = this.entries.map((entry) => (entry.path === path ? renamed : entry))
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
    this.busy = true
    this.creationError = null
    try {
      await this.client.delete(path)
      const selected = this.selected
      if (selected && (selected.path === path || selected.path.startsWith(`${path}/`))) {
        this.clearSelected()
      }
      this.entries = this.entries.filter((entry) => entry.path !== path)
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
    this.active = false
  }

  private refresh(path: string) {
    this.refreshFolder = { path, version: (this.refreshFolder?.version ?? 0) + 1 }
  }
}
