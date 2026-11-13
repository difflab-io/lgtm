/** Directory refresh trigger state for `TreeView`. */
export interface FolderRefresh {
  path: string
  version: number
}

/** Open file or diagnostics buffer tab in `WorkspaceService`. */
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
