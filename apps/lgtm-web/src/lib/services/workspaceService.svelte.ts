import type {
  TreeNode,
  ContextMenuItem,
  CodeEditorCompletionItem,
  CodeEditorHoverResult,
  CodeEditorSignatureHelp,
  TabItem,
} from 'lgtm-ui'
import { WorkspaceNodeViewModel } from '../models/workspaceNodeViewModel'
import {
  errorMessage,
  type WorkspaceClient,
  type WorkspaceNode,
  type FileContent,
  type LspDiagnostic,
  type LspFileDiagnosticsEvent,
  type LspServerStatusEvent,
  type Project,
  type Session,
  type SidebarWidget,
  type TabCandidate,
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
  edited?: boolean
}

/** Form and submission state for the `New Worktree` dialog in `ProjectsPane`. */
export interface WorktreeDialogState {
  projectId: string
  projectName: string
  branch: string
  baseRef: string
  submitting: boolean
  error: string | null
}

/** Confirmation and force-retry state for the `Remove Worktree` dialog in `ProjectsPane`. */
export interface RemoveWorktreeConfirmState {
  projectId: string
  projectName: string
  worktreePath: string
  worktreeName: string
  source: 'managed' | 'external'
  requiresForce: boolean
  submitting: boolean
  error: string | null
}

interface CachedWorkspaceSnapshot {
  root: string
  entries: WorkspaceNode[]
  listed: Record<string, WorkspaceNode>
  tabs: WorkspaceTab[]
  activeTabId: string | null
  selected: WorkspaceNode | null
  fileContent: FileContent | null
  selectionOffset: number | null
  expandedDirs: string[]
}

const ADJECTIVES = [
  'amber',
  'brisk',
  'calm',
  'clear',
  'crisp',
  'focal',
  'lunar',
  'quiet',
  'rapid',
  'solar',
]
const NOUNS = [
  'arch',
  'brook',
  'cedar',
  'crest',
  'delta',
  'field',
  'grove',
  'haven',
  'path',
  'ridge',
]

function generateDefaultBranchName(): string {
  const adj = ADJECTIVES[Math.floor(Math.random() * ADJECTIVES.length)] ?? 'brisk'
  const noun = NOUNS[Math.floor(Math.random() * NOUNS.length)] ?? 'grove'
  const hex = Math.floor(Math.random() * 0xffff)
    .toString(16)
    .padStart(4, '0')
  return `worktree/${adj}-${noun}-${hex}`
}

/** Converts a Git branch name into a filesystem-safe directory slug matching `lgtm_rsutils::paths::branch_to_path_slug`. */
export function branchToPathSlug(branch: string): string {
  const trimmed = branch.trim()
  if (!trimmed) return ''
  const stripped = trimmed.startsWith('worktree/') ? trimmed.slice('worktree/'.length) : trimmed
  let out = ''
  let lastWasDash = false
  for (const ch of stripped) {
    if (/^[a-zA-Z0-9_.]$/.test(ch)) {
      out += ch.toLowerCase()
      lastWasDash = false
    } else if (!lastWasDash && out.length > 0) {
      out += '-'
      lastWasDash = true
    }
  }
  const clean = out.replace(/-+$/g, '')
  return clean || 'worktree'
}

/** Workspace view model coordinating file tree browsing, projects/worktrees, open tabs, debounced saves, LSP state, and settings. */
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
  sidebarWidget = $state<SidebarWidget>('explorer')
  projects = $state<Project[]>([])
  expandedDirs = $state<string[]>([])
  worktreeDialog = $state<WorktreeDialogState | null>(null)
  removeWorktreeConfirm = $state<RemoveWorktreeConfirmState | null>(null)
  private listed = $state<Record<string, WorkspaceNode>>({})
  private workspaceCache = new Map<string, CachedWorkspaceSnapshot>()
  private sessionHydratedRoots = new Set<string>()
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
    const projectItems: { label: string; value: string }[] = []
    const seen = new Set<string>()
    for (const project of this.projects) {
      if (!seen.has(project.rootPath)) {
        seen.add(project.rootPath)
        projectItems.push({
          label:
            project.rootPath === this.defaultRoot || this.isDefaultWorkspaceRoot(project.rootPath)
              ? 'Local workspace'
              : project.name,
          value: project.rootPath,
        })
      }
      for (const wt of project.worktrees ?? []) {
        if (!seen.has(wt.path)) {
          seen.add(wt.path)
          projectItems.push({
            label: `${project.name} / ${wt.name}`,
            value: wt.path,
          })
        }
      }
    }
    const roots = this.settings.recentRoots.length ? this.settings.recentRoots : [this.root]
    for (const path of roots) {
      if (!seen.has(path)) {
        seen.add(path)
        projectItems.push({
          label:
            path === this.defaultRoot || this.isDefaultWorkspaceRoot(path)
              ? 'Local workspace'
              : (path.split(/[\\/]/).filter(Boolean).at(-1) ?? path),
          value: path,
        })
      }
    }
    return [...projectItems, { label: 'Open folder…', value: '__open_folder__' }]
  }

  /** Returns the project that owns the currently active workspace root, if any. */
  get activeProject(): Project | null {
    return (
      this.projects.find(
        (p) => p.rootPath === this.root || (p.worktrees ?? []).some((wt) => wt.path === this.root),
      ) ??
      this.projects[0] ??
      null
    )
  }

  /** Builds `TreeNode` items for `ProjectsPane` with collapsed-branch active-worktree filtering. */
  get projectNodes(): TreeNode[] {
    return this.projects.map((project) => {
      const allWorktrees = project.worktrees ?? []
      const visibleWorktrees = project.collapsed
        ? allWorktrees.filter((wt) => wt.path === this.root)
        : allWorktrees
      return {
        value: project.rootPath,
        label: project.name,
        description: project.branch ?? project.rootPath,
        branch: allWorktrees.length > 0,
        children: visibleWorktrees.map((wt) => ({
          value: wt.path,
          label: wt.name,
          description: wt.branch ? `${wt.branch} · ${wt.source}` : wt.source,
          branch: false,
        })),
      }
    })
  }

  /** Returns the list of expanded project root paths for `ProjectsPane`. */
  get projectExpandedValues(): string[] {
    return this.projects.filter((project) => !project.collapsed).map((project) => project.rootPath)
  }

  /** Returns the live `~/.lgtm/worktrees/<project>/<branch-slug>` preview path for the `New Worktree` dialog. */
  get worktreePreviewPath(): string {
    if (!this.worktreeDialog) return ''
    const slug = branchToPathSlug(this.worktreeDialog.branch)
    return `~/.lgtm/worktrees/${this.worktreeDialog.projectName}/${slug || '<branch-slug>'}`
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

  /** Switches the active left sidebar widget between `explorer` and `projects` and persists the choice. */
  setSidebarWidget(widget: SidebarWidget) {
    if (this.sidebarWidget === widget) return
    this.sidebarWidget = widget
    void this.saveCurrentWorkspaceSession()
  }

  /** Updates the tracked list of expanded directory paths in `WorkspaceExplorer` and persists session state. */
  setExpandedDirs(dirs: string[]) {
    this.expandedDirs = [...dirs]
    void this.saveCurrentWorkspaceSession()
  }

  /** Synchronizes `ProjectsPane` branch expansion changes with `SessionService::set_project_collapsed`. */
  async handleProjectExpandedChange(nextExpanded: string[]) {
    const expandedSet = new Set(nextExpanded)
    for (const project of this.projects) {
      if ((project.worktrees ?? []).length === 0) continue
      const nextCollapsed = !expandedSet.has(project.rootPath)
      if (project.collapsed !== nextCollapsed) {
        await this.setProjectCollapsed(project.id, nextCollapsed)
      }
    }
  }

  /** Sets the collapsed state for `projectId` and persists it to the session file. */
  async setProjectCollapsed(projectId: string, collapsed: boolean) {
    this.projects = this.projects.map((p) => (p.id === projectId ? { ...p, collapsed } : p))
    if (!this.client.projectSetCollapsed) return
    try {
      const session = await this.client.projectSetCollapsed(projectId, collapsed)
      if (this.active) this.applySession(session)
    } catch (error) {
      if (this.active) this.creationError = errorMessage(error)
    }
  }

  /** Opens the `New Worktree` dialog for `projectId` (or the active project). */
  openNewWorktreeDialog(projectId?: string) {
    const project = projectId
      ? this.projects.find((p) => p.id === projectId || p.rootPath === projectId)
      : this.activeProject
    if (!project) return
    this.worktreeDialog = {
      projectId: project.id,
      projectName: project.name,
      branch: generateDefaultBranchName(),
      baseRef: project.branch ?? 'HEAD',
      submitting: false,
      error: null,
    }
  }

  /** Closes the `New Worktree` dialog. */
  closeNewWorktreeDialog() {
    this.worktreeDialog = null
  }

  /** Creates a managed Git worktree from the `New Worktree` dialog and switches to it on success. */
  async submitNewWorktree() {
    const dialog = this.worktreeDialog
    if (!dialog || dialog.submitting || !this.client.projectCreateWorktree) return
    const branch = dialog.branch.trim()
    if (!branch) {
      this.worktreeDialog = { ...dialog, error: 'Branch name cannot be empty.' }
      return
    }
    this.worktreeDialog = { ...dialog, submitting: true, error: null }
    try {
      const session = await this.client.projectCreateWorktree(
        dialog.projectId,
        branch,
        dialog.baseRef.trim() || undefined,
      )
      if (!this.active) return
      this.worktreeDialog = null
      this.applySession(session)
      if (session.activeWorkspaceRoot && session.activeWorkspaceRoot !== this.root) {
        await this.open(session.activeWorkspaceRoot)
      }
    } catch (error) {
      if (this.active && this.worktreeDialog) {
        this.worktreeDialog = {
          ...this.worktreeDialog,
          submitting: false,
          error: errorMessage(error),
        }
      }
    }
  }

  /** Opens the removal confirmation modal for `worktreePath` under `projectId` (or detaches an external worktree). */
  promptRemoveWorktree(projectId: string, worktreePath: string) {
    const project = this.projects.find((p) => p.id === projectId || p.rootPath === projectId)
    const worktree = project?.worktrees.find((wt) => wt.path === worktreePath)
    if (!project || !worktree) return
    this.removeWorktreeConfirm = {
      projectId: project.id,
      projectName: project.name,
      worktreePath: worktree.path,
      worktreeName: worktree.name,
      source: worktree.source,
      requiresForce: false,
      submitting: false,
      error: null,
    }
  }

  /** Cancels the pending worktree removal confirmation dialog. */
  cancelRemoveWorktree() {
    this.removeWorktreeConfirm = null
  }

  /** Executes worktree removal (`Managed` deletes `~/.lgtm/worktrees/...`, `External` detaches from session). */
  async confirmRemoveWorktree(force = false) {
    const confirm = this.removeWorktreeConfirm
    if (!confirm || confirm.submitting || !this.client.projectRemoveWorktree) return
    this.removeWorktreeConfirm = { ...confirm, submitting: true, error: null }
    try {
      const session = await this.client.projectRemoveWorktree(
        confirm.projectId,
        confirm.worktreePath,
        force || confirm.requiresForce,
      )
      if (!this.active) return
      this.workspaceCache.delete(confirm.worktreePath)
      this.sessionHydratedRoots.delete(confirm.worktreePath)
      this.removeWorktreeConfirm = null
      this.applySession(session)
      if (session.activeWorkspaceRoot && session.activeWorkspaceRoot !== this.root) {
        await this.open(session.activeWorkspaceRoot)
      }
    } catch (error) {
      if (!this.active || !this.removeWorktreeConfirm) return
      const msg = errorMessage(error)
      const isDirty =
        (typeof error === 'object' &&
          error !== null &&
          'kind' in error &&
          (error as { kind?: string }).kind === 'dirtyWorktree') ||
        msg.toLowerCase().includes('untracked') ||
        msg.toLowerCase().includes('modified') ||
        msg.toLowerCase().includes('force')
      this.removeWorktreeConfirm = {
        ...this.removeWorktreeConfirm,
        submitting: false,
        requiresForce: isDirty || this.removeWorktreeConfirm.requiresForce,
        error: msg,
      }
    }
  }

  /** Detaches `projectId` from the session without deleting files on disk. */
  async removeProject(projectId: string) {
    if (!this.client.projectRemove) return
    const project = this.projects.find((p) => p.id === projectId || p.rootPath === projectId)
    if (!project) return
    try {
      const session = await this.client.projectRemove(project.id)
      if (!this.active) return
      this.workspaceCache.delete(project.rootPath)
      this.sessionHydratedRoots.delete(project.rootPath)
      this.applySession(session)
      if (session.activeWorkspaceRoot && session.activeWorkspaceRoot !== this.root) {
        await this.open(session.activeWorkspaceRoot)
      }
    } catch (error) {
      if (this.active) this.creationError = errorMessage(error)
    }
  }

  /** Builds right-click `ContextMenu` items for a project or worktree node in `ProjectsPane`. */
  projectContextItems(node: TreeNode): ContextMenuItem[] {
    const project = this.projects.find((p) => p.rootPath === node.value)
    if (project) {
      return [
        { value: 'new-worktree', label: 'New Worktree' },
        { value: 'open-primary', label: 'Open Primary Checkout' },
        ...((project.worktrees ?? []).length > 0
          ? [
              {
                value: project.collapsed ? 'expand-worktrees' : 'collapse-worktrees',
                label: project.collapsed ? 'Expand Worktrees' : 'Collapse Worktrees',
              },
            ]
          : []),
        { value: 'remove-project', label: 'Remove Project', destructive: true },
      ]
    }
    return [
      { value: 'open-worktree', label: 'Open Worktree' },
      { value: 'remove-worktree', label: 'Remove Worktree', destructive: true },
    ]
  }

  /** Handles context menu and hotkey actions on project and worktree nodes in `ProjectsPane`. */
  async projectContextAction(value: string, action: string) {
    const directProject = this.projects.find((p) => p.rootPath === value)
    const ownerProject =
      directProject ??
      this.projects.find((p) => (p.worktrees ?? []).some((wt) => wt.path === value))
    if (action === 'new-worktree' || action === 'new-file' || action === 'new-folder') {
      this.openNewWorktreeDialog(ownerProject?.id)
      return
    }
    if (action === 'open-primary' || action === 'open-worktree') {
      await this.selectWorkspace(value)
      return
    }
    if (action === 'collapse-worktrees' && directProject) {
      await this.setProjectCollapsed(directProject.id, true)
      return
    }
    if (action === 'expand-worktrees' && directProject) {
      await this.setProjectCollapsed(directProject.id, false)
      return
    }
    if (action === 'remove-project' && directProject) {
      await this.removeProject(directProject.id)
      return
    }
    if (action === 'remove-worktree' && ownerProject && !directProject) {
      this.promptRemoveWorktree(ownerProject.id, value)
      return
    }
    if (action === 'delete') {
      if (directProject) {
        await this.removeProject(directProject.id)
      } else if (ownerProject) {
        this.promptRemoveWorktree(ownerProject.id, value)
      }
    }
  }

  /** Applies an incoming `Session` snapshot from the backend. */
  applySession(session: Session) {
    if (!this.active) return
    this.projects = session.projects ?? []
    this.sidebarWidget = session.sidebarWidget ?? 'explorer'
  }

  /** Synchronizes session state with the backend and optionally hydrates persisted tabs for `this.root`. */
  async syncSession(hydrateTabs = false) {
    if (!this.client.sessionGet) return
    try {
      const session = await this.client.sessionGet()
      if (!this.active) return
      this.applySession(session)
      if (hydrateTabs && !this.sessionHydratedRoots.has(this.root)) {
        this.sessionHydratedRoots.add(this.root)
        const wsState = session.workspaces?.[this.root]
        if (wsState) {
          this.expandedDirs = [...(wsState.expandedDirs ?? [])]
          if (this.tabs.length === 0 && (wsState.tabs ?? []).length > 0) {
            await this.restorePersistedTabs(wsState.tabs)
          }
        }
      }
    } catch {
      // Session commands are optional in lightweight unit test mocks.
    }
  }

  /** Saves the active workspace's filtered tab candidates (`edited || lastOpened`) and expanded directories. */
  async saveCurrentWorkspaceSession(targetRoot = this.root) {
    if (!this.client.sessionSave || !targetRoot || this.isDefaultPlaceholderRoot(targetRoot)) {
      return
    }
    const snapshot =
      targetRoot === this.root
        ? this.buildSnapshotForCurrentRoot()
        : this.workspaceCache.get(targetRoot)
    if (!snapshot) return
    const candidates: TabCandidate[] = snapshot.tabs
      .filter((tab) => tab.kind === 'file' && Boolean(tab.path))
      .map((tab) => ({
        path: tab.path!,
        title: tab.title,
        edited: Boolean(tab.edited),
        lastOpened: tab.id === snapshot.activeTabId,
      }))
    try {
      const session = await this.client.sessionSave({
        workspaceRoot: targetRoot,
        tabs: candidates,
        expandedDirs: snapshot.expandedDirs,
        sidebarWidget: this.sidebarWidget,
      })
      if (this.active) this.applySession(session)
    } catch {
      // Ignore background session save failures.
    }
  }

  /** Opens the default or specified workspace directory and refreshes settings, session, and LSP state. */
  async open(path?: string) {
    const outgoingRoot = this.root && !this.isDefaultPlaceholderRoot(this.root) ? this.root : null
    if (outgoingRoot && path && path !== outgoingRoot) {
      this.saveCurrentWorkspaceToCache(outgoingRoot)
    }
    await this.flushSave()
    if (outgoingRoot && path && path !== outgoingRoot) {
      void this.saveCurrentWorkspaceSession(outgoingRoot)
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
        this.listed = switchedCached.listed
        this.tabs = switchedCached.tabs
        this.activeTabId = switchedCached.activeTabId
        this.selected = switchedCached.selected
        this.fileContent = switchedCached.fileContent
        this.selectionOffset = switchedCached.selectionOffset
        this.expandedDirs = switchedCached.expandedDirs
      } else if (!fileOpenedDuringInit) {
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
      void this.syncLspState()
      await this.syncSession(!switchedCached && !fileOpenedDuringInit)
      this.saveCurrentWorkspaceToCache(this.root)
    } catch (error) {
      if (this.active) this.error = errorMessage(error)
    } finally {
      if (this.active) this.loading = false
    }
  }

  /** Opens `path` in an editor tab with an optional custom tab `title`, cursor `offset`, and editor `focus`. */
  async openPath(path: string, options?: { title?: string; offset?: number; focus?: boolean }) {
    const entry = this.resolveFileEntry(path, options?.title)
    if (entry.kind === 'file') {
      await this.openFile(entry, options?.offset, options?.focus)
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
    await this.openPath(path)
  }

  /** Reads a file from the workspace, opens or activates its tab, and optionally positions the cursor at `targetOffset`. */
  async openFile(entry: WorkspaceNode, targetOffset?: number, focus = false) {
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
      void this.saveCurrentWorkspaceSession()
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
    if (this.activeTabId !== tabId) {
      this.saveCurrentWorkspaceToCache(this.root)
      void this.saveCurrentWorkspaceSession()
      return
    }
    const fallback = remaining[index] ?? remaining[index - 1] ?? null
    if (!fallback) {
      this.clearSelected()
      this.saveCurrentWorkspaceToCache(this.root)
      void this.saveCurrentWorkspaceSession()
      return
    }
    this.activeTabId = null
    await this.activateTab(fallback.id)
  }

  /** Closes the currently active editor tab, if any. */
  async closeActiveTab() {
    if (!this.activeTabId) return
    await this.closeTab(this.activeTabId)
  }

  /** Cycles forward through open tabs by `count`. */
  async selectNextTab(count = 1) {
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
  async selectPrevTab(count = 1) {
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
  async reloadCleanConfigTabs() {
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

  /** Updates the active editor buffer, marks the tab as `edited`, notifies LSP `didChange`, and schedules a 150ms debounced save. */
  updateContent(content: string) {
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
            tab.id === tabId && tab.content === content
              ? { ...tab, dirty: false, edited: true }
              : tab,
          )
          if (this.selected?.path === path && this.fileContent?.content === content) {
            this.fileContent = {
              ...this.fileContent,
              ...saved,
              languageId: saved.languageId ?? this.fileContent.languageId,
              lspStatus: saved.lspStatus ?? this.fileContent.lspStatus,
            }
          }
          this.saveCurrentWorkspaceToCache(this.root)
          void this.saveCurrentWorkspaceSession()
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

  /** Flushes any dirty buffer and session state, and marks the service inactive on component teardown. */
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
                revision: 'cached',
                languageId: activeTab.languageId ?? null,
              }
            : null,
      selectionOffset: this.selectionOffset,
      expandedDirs: [...this.expandedDirs],
    }
  }

  private saveCurrentWorkspaceToCache(root: string) {
    if (!root || this.isDefaultPlaceholderRoot(root)) return
    this.workspaceCache.set(root, this.buildSnapshotForCurrentRoot())
  }

  private applyCachedWorkspace(cached: CachedWorkspaceSnapshot) {
    this.loading = false
    this.error = null
    this.creationError = null
    this.openingError = null
    this.pendingEntry = null
    this.pendingParent = ''
    this.root = cached.root
    this.entries = cached.entries
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

  private async refreshWorkspaceInBackground(path: string) {
    try {
      const result = await this.client.open(path)
      if (!this.active || this.root !== path) return
      this.root = result.root
      this.settings.rememberRoot(result.root)
      this.entries = result.entries
      void this.settings.load()
      void this.syncLspState()
      await this.syncSession(false)
      this.saveCurrentWorkspaceToCache(this.root)
    } catch (error) {
      if (this.active && this.root === path) {
        this.error = errorMessage(error)
      }
    }
  }

  private async restorePersistedTabs(
    persistedTabs: { path: string; title: string; edited: boolean; lastOpened: boolean }[],
  ) {
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

  private upsertTab(tab: WorkspaceTab) {
    const existingIdx = this.tabs.findIndex((item) => item.id === tab.id)
    if (existingIdx === -1) {
      this.tabs = [...this.tabs, tab]
    } else {
      this.tabs = this.tabs.map((item, idx) => (idx === existingIdx ? tab : item))
    }
  }
}
