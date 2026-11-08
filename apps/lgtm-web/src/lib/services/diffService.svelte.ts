import type { DiffEditorMode, TreeNode } from 'lgtm-ui'
import {
  errorMessage,
  type DiffCommitItem,
  type DiffFileEntry,
  type DiffWorkspaceSnapshot,
  type FileDiffContent,
  type WorkspaceClient,
} from '../clients/workspaceClient'

interface FolderTrieNode {
  name: string
  path: string
  folders: Map<string, FolderTrieNode>
  files: DiffFileEntry[]
}

/** Builds a collapsible folder `TreeNode[]` hierarchy with compacted single-child folder chains and returns all folder branch IDs. */
export function buildDiffFileTree(files: DiffFileEntry[]): {
  nodes: TreeNode[]
  folderIds: string[]
  orderedPaths: string[]
} {
  const root: FolderTrieNode = {
    name: '',
    path: '',
    folders: new Map(),
    files: [],
  }

  for (const file of files) {
    const segments = file.path.split('/').filter(Boolean)
    if (segments.length === 0) continue
    let cursor = root
    for (let i = 0; i < segments.length - 1; i++) {
      const seg = segments[i]
      const nextPath = cursor.path ? `${cursor.path}/${seg}` : seg
      let child = cursor.folders.get(seg)
      if (!child) {
        child = {
          name: seg,
          path: nextPath,
          folders: new Map(),
          files: [],
        }
        cursor.folders.set(seg, child)
      }
      cursor = child
    }
    cursor.files.push(file)
  }

  const folderIds: string[] = []
  const orderedPaths: string[] = []

  const compactFolder = (folder: FolderTrieNode): FolderTrieNode => {
    let current = folder
    let combinedName = folder.name
    while (current.folders.size === 1 && current.files.length === 0) {
      const next = Array.from(current.folders.values())[0]
      combinedName = `${combinedName}/${next.name}`
      current = next
    }
    return {
      name: combinedName,
      path: current.path,
      folders: current.folders,
      files: current.files,
    }
  }

  const toTreeNodes = (folder: FolderTrieNode): TreeNode[] => {
    const sortedFolders = Array.from(folder.folders.values())
      .map(compactFolder)
      .sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))

    const branchNodes: TreeNode[] = sortedFolders.map((sub) => {
      folderIds.push(sub.path)
      return {
        value: sub.path,
        label: sub.name,
        branch: true,
        children: toTreeNodes(sub),
      }
    })

    const sortedFiles = [...folder.files].sort((a, b) => {
      const aName = a.path.split('/').at(-1) ?? a.path
      const bName = b.path.split('/').at(-1) ?? b.path
      return aName.toLowerCase().localeCompare(bName.toLowerCase())
    })

    const fileNodes: TreeNode[] = sortedFiles.map((entry) => {
      orderedPaths.push(entry.path)
      const baseName = entry.path.split('/').at(-1) ?? entry.path
      const label =
        entry.status === 'renamed' && entry.oldPath
          ? `${entry.oldPath.split('/').at(-1) ?? entry.oldPath} → ${baseName}`
          : baseName
      return {
        value: entry.path,
        label,
      }
    })

    return [...branchNodes, ...fileNodes]
  }

  const nodes = toTreeNodes(root)
  return { nodes, folderIds, orderedPaths }
}

/** Reactive frontend service managing the commit checklist, changed-file tree, active file diff buffer, and diff display modes. */
export class DiffService {
  /** Checked-out Git branch name. */
  branch = $state('')
  /** Ordered newest-first commit checklist items. */
  commits = $state<DiffCommitItem[]>([])
  /** Currently checked commit IDs in the bottom checklist. */
  selectedCommitIds = $state<string[]>([])
  /** Changed file entries for the selected commit range. */
  files = $state<DiffFileEntry[]>([])
  /** Currently selected workspace-relative file path in the diff explorer. */
  activeFilePath = $state<string | null>(null)
  /** Loaded `(baseContent, headContent)` payload for `activeFilePath`. */
  activeDiff = $state<FileDiffContent | null>(null)
  /** In-memory working-tree draft text when the user edits the writable head pane. */
  draftHeadContent = $state<string | null>(null)
  /** Active diff layout mode (`split` by default). */
  viewMode = $state<DiffEditorMode>('split')
  /** Whether the full uncollapsed file is shown (`false` collapses unchanged blocks into hunks). */
  showFullFile = $state(false)
  /** Whether a diff state or file load is in flight. */
  isLoading = $state(false)
  /** Error message from the most recent diff operation, if any. */
  error = $state<string | null>(null)
  /** Monotonic counter incremented when a new file diff payload is loaded. */
  diffVersion = $state(0)
  private requestSeq = 0

  /** Constructs a `DiffService` bound to `client` and an optional mode-sync callback. */
  constructor(
    private client: WorkspaceClient,
    private onModeSynced?: (snapshot: DiffWorkspaceSnapshot) => void,
  ) {}

  /** Hierarchical collapsible folder and file tree built from `files`. */
  get treeState(): { nodes: TreeNode[]; folderIds: string[]; orderedPaths: string[] } {
    return buildDiffFileTree(this.files)
  }

  /** Hierarchical `TreeNode[]` passed to `DiffExplorer`'s `<TreeView>`. */
  get treeNodes(): TreeNode[] {
    return this.treeState.nodes
  }

  /** Folder branch IDs expanded by default in `DiffExplorer`. */
  get expandedFolders(): string[] {
    return this.treeState.folderIds
  }

  /** Map of workspace-relative file paths to their `DiffFileEntry` metadata. */
  get fileMap(): Record<string, DiffFileEntry> {
    const map: Record<string, DiffFileEntry> = {}
    for (const file of this.files) {
      map[file.path] = file
    }
    return map
  }

  /** Returns the `DiffFileEntry` for `path`, or `undefined` if `path` is a folder branch. */
  getFileEntry(path: string): DiffFileEntry | undefined {
    return this.fileMap[path]
  }

  /** Total additions across all changed files in the current selection. */
  get totalAdditions(): number {
    return this.files.reduce((sum, file) => sum + file.additions, 0)
  }

  /** Total deletions across all changed files in the current selection. */
  get totalDeletions(): number {
    return this.files.reduce((sum, file) => sum + file.deletions, 0)
  }

  /** Resets all diff state when switching workspaces. */
  reset(): void {
    this.branch = ''
    this.commits = []
    this.selectedCommitIds = []
    this.files = []
    this.activeFilePath = null
    this.activeDiff = null
    this.draftHeadContent = null
    this.error = null
  }

  /** Refreshes the full workspace diff snapshot from the desktop backend. */
  async refresh(): Promise<void> {
    if (!this.client.isDesktop() || !this.client.diffGetState) {
      return
    }
    this.isLoading = true
    this.error = null
    try {
      const snapshot = await this.client.diffGetState()
      this.onModeSynced?.(snapshot)
      await this.applySnapshot(snapshot)
    } catch (err) {
      this.error = errorMessage(err)
    } finally {
      this.isLoading = false
    }
  }

  /** Applies a `DiffWorkspaceSnapshot` and loads the active or first changed file's diff content. */
  async applySnapshot(snapshot: DiffWorkspaceSnapshot): Promise<void> {
    this.branch = snapshot.branch
    this.commits = snapshot.commits
    this.selectedCommitIds = snapshot.selectedCommitIds
    this.files = snapshot.files

    const ordered = this.treeState.orderedPaths
    const nextPath =
      this.activeFilePath && ordered.includes(this.activeFilePath)
        ? this.activeFilePath
        : (ordered[0] ?? null)

    if (nextPath) {
      await this.loadFileDiff(nextPath)
    } else {
      this.activeFilePath = null
      this.activeDiff = null
      this.draftHeadContent = null
    }
  }

  /** Toggles `commitId` in the commit checklist and recomputes the changed file list and active diff. */
  async toggleCommit(commitId: string): Promise<void> {
    const exists = this.selectedCommitIds.includes(commitId)
    let nextIds: string[]
    if (exists) {
      if (this.selectedCommitIds.length <= 1) {
        return
      }
      nextIds = this.selectedCommitIds.filter((id) => id !== commitId)
    } else {
      nextIds = [...this.selectedCommitIds, commitId]
    }
    await this.selectCommitRange(nextIds)
  }

  /** Selects an explicit set of commit IDs and reloads the changed file tree and active diff. */
  async selectCommitRange(commitIds: string[]): Promise<void> {
    if (commitIds.length === 0) return
    this.selectedCommitIds = commitIds
    if (!this.client.isDesktop() || !this.client.diffListFiles) return
    this.isLoading = true
    this.error = null
    try {
      const files = await this.client.diffListFiles(commitIds)
      this.files = files
      const ordered = this.treeState.orderedPaths
      const nextPath =
        this.activeFilePath && ordered.includes(this.activeFilePath)
          ? this.activeFilePath
          : (ordered[0] ?? null)
      if (nextPath) {
        await this.loadFileDiff(nextPath)
      } else {
        this.activeFilePath = null
        this.activeDiff = null
        this.draftHeadContent = null
      }
    } catch (err) {
      this.error = errorMessage(err)
    } finally {
      this.isLoading = false
    }
  }

  /** Selects a changed file by its workspace-relative path and loads its `(baseContent, headContent)` diff. */
  async selectFile(path: string): Promise<void> {
    if (!this.fileMap[path]) {
      return
    }
    await this.loadFileDiff(path)
  }

  /** Selects the next changed file in tree order. */
  async selectNextFile(): Promise<void> {
    const ordered = this.treeState.orderedPaths
    if (ordered.length === 0) return
    const currentIndex = this.activeFilePath ? ordered.indexOf(this.activeFilePath) : -1
    const nextIndex = (currentIndex + 1) % ordered.length
    await this.loadFileDiff(ordered[nextIndex])
  }

  /** Selects the previous changed file in tree order. */
  async selectPrevFile(): Promise<void> {
    const ordered = this.treeState.orderedPaths
    if (ordered.length === 0) return
    const currentIndex = this.activeFilePath ? ordered.indexOf(this.activeFilePath) : 0
    const prevIndex = (currentIndex - 1 + ordered.length) % ordered.length
    await this.loadFileDiff(ordered[prevIndex])
  }

  /** Toggles between `split` and `unified` diff modes. */
  toggleViewMode(): void {
    this.viewMode = this.viewMode === 'split' ? 'unified' : 'split'
  }

  /** Sets the active diff view mode (`split` or `unified`). */
  setViewMode(mode: DiffEditorMode): void {
    this.viewMode = mode
  }

  /** Toggles between collapsed diff hunks (`false`) and the full uncollapsed file (`true`). */
  toggleFullFile(): void {
    this.showFullFile = !this.showFullFile
  }

  /** Sets whether the full uncollapsed file is shown. */
  setShowFullFile(show: boolean): void {
    this.showFullFile = show
  }

  /** Updates the in-memory working-tree draft when editing the writable head pane. */
  updateDraft(content: string): void {
    this.draftHeadContent = content
  }

  private async loadFileDiff(path: string): Promise<void> {
    this.activeFilePath = path
    if (!this.client.isDesktop() || !this.client.diffReadFile) return
    const seq = ++this.requestSeq
    try {
      const diffContent = await this.client.diffReadFile(path, this.selectedCommitIds)
      if (seq !== this.requestSeq) return
      this.activeDiff = diffContent
      this.draftHeadContent = diffContent.headContent
      this.diffVersion++
    } catch (err) {
      if (seq === this.requestSeq) {
        this.error = errorMessage(err)
      }
    }
  }
}
