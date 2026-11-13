import type { FileContent, WorkspaceNode } from '../clients/workspaceClient'
import type { WorkspaceTab } from './workspace'

/** In-memory snapshot of an open workspace's file tree, tabs, and selection state for instant workspace switching. */
export interface CachedWorkspaceSnapshot {
  root: string
  entries: WorkspaceNode[]
  dirEntries: Record<string, WorkspaceNode[]>
  listed: Record<string, WorkspaceNode>
  tabs: WorkspaceTab[]
  activeTabId: string | null
  selected: WorkspaceNode | null
  fileContent: FileContent | null
  selectionOffset: number | null
  expandedDirs: string[]
}
