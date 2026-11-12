import type { ContextMenuItem, TreeNode } from 'lgtm-ui'
import {
  errorMessage,
  type Project,
  type Session,
  type WorkspaceClient,
} from '../clients/workspaceClient'

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
  requiresForce: boolean
  submitting: boolean
  error: string | null
}

/** Form state for customizing a project's name, icon, and color in `ProjectsPane`. */
export interface CustomizeProjectDialogState {
  projectId: string
  projectName: string
  icon: string
  color: string
  submitting: boolean
  error: string | null
}

/** Callbacks coordinating project/worktree mutations with `WorkspaceService` and `SessionService`. */
export interface ProjectServiceCallbacks {
  getRoot: () => string
  isActive: () => boolean
  applySession: (session: Session) => void
  openWorkspace: (path: string) => Promise<void>
  selectWorkspace: (path: string) => Promise<void>
  evictWorkspace: (path: string) => void
  setCreationError: (message: string) => void
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

/** Frontend project and managed Git worktree service backing `ProjectsPane` and project modal dialogs. */
export class ProjectService {
  projects = $state<Project[]>([])
  worktreeDialog = $state<WorktreeDialogState | null>(null)
  removeWorktreeConfirm = $state<RemoveWorktreeConfirmState | null>(null)
  customizeProjectDialog = $state<CustomizeProjectDialogState | null>(null)
  private pendingCollapsed = new Map<string, { seq: number; collapsed: boolean }>()
  private collapseRequestSeq = 0
  private collapseQueue: Promise<void> = Promise.resolve()

  /** Creates a `ProjectService` bound to `client` and workspace callbacks. */
  constructor(
    private client: WorkspaceClient,
    private callbacks: ProjectServiceCallbacks,
  ) {}

  /** Returns the project that owns the currently active workspace root, if any. */
  get activeProject(): Project | null {
    const root = this.callbacks.getRoot()
    return (
      this.projects.find(
        (p) => p.rootPath === root || (p.worktrees ?? []).some((wt) => wt.path === root),
      ) ??
      this.projects[0] ??
      null
    )
  }

  /** Builds `TreeNode` items for `ProjectsPane`. */
  get projectNodes(): TreeNode[] {
    return this.projects.map((project) => {
      const allWorktrees = project.worktrees ?? []
      return {
        value: project.rootPath,
        label: project.name,
        description: project.branch ?? project.rootPath,
        branch: allWorktrees.length > 0,
        renamable: false,
        children: allWorktrees.map((wt) => ({
          value: wt.path,
          label: wt.name,
          description: wt.branch ?? wt.path,
          branch: false,
          renamable: true,
        })),
      }
    })
  }

  /** Returns the list of expanded project root paths for `ProjectsPane`. */
  get projectExpandedValues(): string[] {
    return this.projects
      .filter((project) => !project.collapsed && (project.worktrees ?? []).length > 0)
      .map((project) => project.rootPath)
  }

  /** Returns the live `~/.lgtm/worktrees/<project>/<branch-slug>` preview path for the `New Worktree` dialog. */
  get worktreePreviewPath(): string {
    if (!this.worktreeDialog) return ''
    const slug = branchToPathSlug(this.worktreeDialog.branch)
    const project = this.projects.find((p) => p.id === this.worktreeDialog?.projectId)
    const dirName =
      project?.rootPath.split(/[\\/]/).filter(Boolean).at(-1) ?? this.worktreeDialog.projectName
    return `~/.lgtm/worktrees/${dirName}/${slug || '<branch-slug>'}`
  }

  /** Updates `this.projects` from a backend `Session` while preserving optimistic collapse state. */
  applySessionProjects(session: Session): void {
    this.projects = (session.projects ?? []).map((p) => {
      const pending = this.pendingCollapsed.get(p.id)
      return pending ? { ...p, collapsed: pending.collapsed } : p
    })
  }

  /** Synchronizes `ProjectsPane` branch expansion changes with `SessionService::set_project_collapsed`. */
  async handleProjectExpandedChange(nextExpanded: string[]): Promise<void> {
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
  async setProjectCollapsed(projectId: string, collapsed: boolean): Promise<void> {
    const seq = ++this.collapseRequestSeq
    this.pendingCollapsed.set(projectId, { seq, collapsed })
    this.projects = this.projects.map((p) => (p.id === projectId ? { ...p, collapsed } : p))
    if (!this.client.projectSetCollapsed) {
      this.pendingCollapsed.delete(projectId)
      return
    }
    const task = this.collapseQueue.then(async () => {
      if (this.pendingCollapsed.get(projectId)?.seq !== seq) return
      try {
        const session = await this.client.projectSetCollapsed!(projectId, collapsed)
        if (this.pendingCollapsed.get(projectId)?.seq === seq) {
          this.pendingCollapsed.delete(projectId)
          if (this.callbacks.isActive()) this.callbacks.applySession(session)
        }
      } catch (error) {
        if (this.pendingCollapsed.get(projectId)?.seq === seq) {
          this.pendingCollapsed.delete(projectId)
          if (this.callbacks.isActive()) this.callbacks.setCreationError(errorMessage(error))
        }
      }
    })
    this.collapseQueue = task.catch(() => {})
    await task
  }

  /** Opens the `New Worktree` dialog for `projectId` (or the active project). */
  openNewWorktreeDialog(projectId?: string): void {
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
  closeNewWorktreeDialog(): void {
    this.worktreeDialog = null
  }

  /** Opens the `Edit Project` dialog for `projectId` (or the active project). */
  openCustomizeProjectDialog(projectId?: string): void {
    const project = projectId
      ? this.projects.find((p) => p.id === projectId || p.rootPath === projectId)
      : this.activeProject
    if (!project) return
    this.customizeProjectDialog = {
      projectId: project.id,
      projectName: project.name,
      icon: project.icon ?? 'box',
      color: project.color ?? 'default',
      submitting: false,
      error: null,
    }
  }

  /** Closes the `Edit Project` dialog. */
  closeCustomizeProjectDialog(): void {
    this.customizeProjectDialog = null
  }

  /** Persists the edited `projectName`, `icon`, and `color` from the `Edit Project` dialog. */
  async submitCustomizeProject(): Promise<void> {
    const dialog = this.customizeProjectDialog
    if (!dialog || dialog.submitting) return
    const nextName = dialog.projectName.trim()
    if (!nextName) {
      this.customizeProjectDialog = { ...dialog, error: 'Project name cannot be empty.' }
      return
    }
    const nextIcon = dialog.icon === 'box' ? null : dialog.icon
    const nextColor = dialog.color === 'default' ? null : dialog.color
    this.projects = this.projects.map((p) =>
      p.id === dialog.projectId ? { ...p, name: nextName, icon: nextIcon, color: nextColor } : p,
    )
    if (!this.client.projectUpdateAppearance) {
      this.customizeProjectDialog = null
      return
    }
    this.customizeProjectDialog = { ...dialog, submitting: true, error: null }
    try {
      const session = await this.client.projectUpdateAppearance(
        dialog.projectId,
        nextName,
        nextIcon,
        nextColor,
      )
      if (!this.callbacks.isActive()) return
      this.customizeProjectDialog = null
      this.callbacks.applySession(session)
    } catch (error) {
      if (this.callbacks.isActive() && this.customizeProjectDialog) {
        this.customizeProjectDialog = {
          ...this.customizeProjectDialog,
          submitting: false,
          error: errorMessage(error),
        }
      }
    }
  }

  /** Renames a worktree section's display label in `ProjectsPane` without changing its Git branch name. */
  async renameProjectWorktree(worktreePath: string, name: string): Promise<void> {
    const trimmed = name.trim()
    if (!trimmed) throw new Error('Worktree name cannot be empty.')
    const project = this.projects.find((p) =>
      (p.worktrees ?? []).some((wt) => wt.path === worktreePath),
    )
    if (!project) throw new Error('Worktree not found.')
    if (this.client.projectRenameWorktree) {
      const session = await this.client.projectRenameWorktree(project.id, worktreePath, trimmed)
      if (this.callbacks.isActive()) this.callbacks.applySession(session)
      return
    }
    this.projects = this.projects.map((p) =>
      p.id === project.id
        ? {
            ...p,
            worktrees: (p.worktrees ?? []).map((wt) =>
              wt.path === worktreePath ? { ...wt, name: trimmed } : wt,
            ),
          }
        : p,
    )
  }

  /** Creates a managed Git worktree from the `New Worktree` dialog and switches to it on success. */
  async submitNewWorktree(): Promise<void> {
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
      if (!this.callbacks.isActive()) return
      this.worktreeDialog = null
      this.callbacks.applySession(session)
      if (session.activeWorkspaceRoot && session.activeWorkspaceRoot !== this.callbacks.getRoot()) {
        await this.callbacks.openWorkspace(session.activeWorkspaceRoot)
      }
    } catch (error) {
      if (this.callbacks.isActive() && this.worktreeDialog) {
        this.worktreeDialog = {
          ...this.worktreeDialog,
          submitting: false,
          error: errorMessage(error),
        }
      }
    }
  }

  /** Opens the removal confirmation modal for `worktreePath` under `projectId`. */
  promptRemoveWorktree(projectId: string, worktreePath: string): void {
    const project = this.projects.find((p) => p.id === projectId || p.rootPath === projectId)
    const worktree = project?.worktrees.find((wt) => wt.path === worktreePath)
    if (!project || !worktree) return
    this.removeWorktreeConfirm = {
      projectId: project.id,
      projectName: project.name,
      worktreePath: worktree.path,
      worktreeName: worktree.name,
      requiresForce: false,
      submitting: false,
      error: null,
    }
  }

  /** Cancels the pending worktree removal confirmation dialog. */
  cancelRemoveWorktree(): void {
    this.removeWorktreeConfirm = null
  }

  /** Executes managed worktree removal (`~/.lgtm/worktrees/...`). */
  async confirmRemoveWorktree(force = false): Promise<void> {
    const confirm = this.removeWorktreeConfirm
    if (!confirm || confirm.submitting || !this.client.projectRemoveWorktree) return
    this.removeWorktreeConfirm = { ...confirm, submitting: true, error: null }
    try {
      const session = await this.client.projectRemoveWorktree(
        confirm.projectId,
        confirm.worktreePath,
        force || confirm.requiresForce,
      )
      if (!this.callbacks.isActive()) return
      this.callbacks.evictWorkspace(confirm.worktreePath)
      this.removeWorktreeConfirm = null
      this.callbacks.applySession(session)
      if (session.activeWorkspaceRoot && session.activeWorkspaceRoot !== this.callbacks.getRoot()) {
        await this.callbacks.openWorkspace(session.activeWorkspaceRoot)
      }
    } catch (error) {
      if (!this.callbacks.isActive() || !this.removeWorktreeConfirm) return
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
  async removeProject(projectId: string): Promise<void> {
    if (!this.client.projectRemove) return
    const project = this.projects.find((p) => p.id === projectId || p.rootPath === projectId)
    if (!project) return
    try {
      const session = await this.client.projectRemove(project.id)
      if (!this.callbacks.isActive()) return
      this.callbacks.evictWorkspace(project.rootPath)
      this.callbacks.applySession(session)
      if (session.activeWorkspaceRoot && session.activeWorkspaceRoot !== this.callbacks.getRoot()) {
        await this.callbacks.openWorkspace(session.activeWorkspaceRoot)
      }
    } catch (error) {
      if (this.callbacks.isActive()) this.callbacks.setCreationError(errorMessage(error))
    }
  }

  /** Builds right-click `ContextMenu` items for a project or worktree node in `ProjectsPane`. */
  projectContextItems(node: TreeNode): ContextMenuItem[] {
    const project = this.projects.find((p) => p.rootPath === node.value)
    if (project) {
      return [
        { value: 'new-worktree', label: 'New Worktree' },
        { value: 'customize-project', label: 'Edit' },
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
      { value: 'rename', label: 'Rename Worktree', shortcut: 'r' },
      { value: 'remove-worktree', label: 'Remove Worktree', destructive: true },
    ]
  }

  /** Handles context menu and hotkey actions on project and worktree nodes in `ProjectsPane`. */
  async projectContextAction(value: string, action: string): Promise<void> {
    const directProject = this.projects.find((p) => p.rootPath === value)
    const ownerProject =
      directProject ??
      this.projects.find((p) => (p.worktrees ?? []).some((wt) => wt.path === value))
    if (action === 'new-worktree' || action === 'new-file' || action === 'new-folder') {
      this.openNewWorktreeDialog(ownerProject?.id)
      return
    }
    if (action === 'customize-project' && ownerProject) {
      this.openCustomizeProjectDialog(ownerProject.id)
      return
    }
    if (action === 'open-primary' || action === 'open-worktree') {
      await this.callbacks.selectWorkspace(value)
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
}
