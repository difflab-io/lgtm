import { invoke, isTauri } from '@tauri-apps/api/core'

export interface WorkspaceNode {
  path: string
  name: string
  kind: 'directory' | 'file' | 'unavailable'
}

export interface FileContent {
  content: string
  revision: string
}

export interface WorkspaceClient {
  open(path?: string): Promise<{ root: string; entries: WorkspaceNode[] }>
  pickFolder(): Promise<string | null>
  list(path: string): Promise<WorkspaceNode[]>
  read(path: string): Promise<FileContent>
  createFile(name: string, parentPath?: string): Promise<WorkspaceNode>
  createFolder(name: string, parentPath?: string): Promise<WorkspaceNode>
  rename(path: string, name: string): Promise<WorkspaceNode>
  delete(path: string): Promise<void>
}

export const workspaceClient: WorkspaceClient = {
  open: (path?: string) => {
    if (!isTauri())
      return Promise.reject(new Error('Open LGTM in the desktop app to access a local workspace.'))
    return invoke('workspace_open', path ? { path } : {})
  },
  pickFolder: () => invoke('workspace_pick_folder'),
  list: (relativePath) => invoke('workspace_list', { relativePath }),
  read: (relativePath) => invoke('workspace_read', { relativePath }),
  createFile: (name, parentPath = '') => invoke('workspace_create_file', { name, parentPath }),
  createFolder: (name, parentPath = '') => invoke('workspace_create_folder', { name, parentPath }),
  rename: (relativePath, name) => invoke('workspace_rename', { relativePath, name }),
  delete: (relativePath) => invoke('workspace_delete', { relativePath }),
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'object' && error && 'message' in error) return String(error.message)
  return String(error)
}
