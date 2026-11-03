import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'

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
  isDesktop(): boolean
  open(path?: string): Promise<{ root: string; entries: WorkspaceNode[] }>
  pickFolder(): Promise<string | null>
  list(path: string): Promise<WorkspaceNode[]>
  read(path: string): Promise<FileContent>
  write(path: string, content: string): Promise<FileContent>
  createFile(name: string, parentPath?: string): Promise<WorkspaceNode>
  createFolder(name: string, parentPath?: string): Promise<WorkspaceNode>
  rename(path: string, name: string): Promise<WorkspaceNode>
  delete(path: string): Promise<void>
  frontendReady(): Promise<void>
  finishQuit(): Promise<void>
  registerQuitHandlers(onBeforeQuit?: () => Promise<void>): Promise<(() => void)[]>
}

export const workspaceClient: WorkspaceClient = {
  isDesktop: () => isTauri(),
  open: (path?: string) => {
    if (!isTauri())
      return Promise.reject(new Error('Open LGTM in the desktop app to access a local workspace.'))
    return invoke('workspace_open', path ? { path } : {})
  },
  pickFolder: () => invoke('workspace_pick_folder'),
  list: (relativePath) => invoke('workspace_list', { relativePath }),
  read: (relativePath) => invoke('workspace_read', { relativePath }),
  write: (relativePath, content) => invoke('workspace_write', { relativePath, content }),
  createFile: (name, parentPath = '') => invoke('workspace_create_file', { name, parentPath }),
  createFolder: (name, parentPath = '') => invoke('workspace_create_folder', { name, parentPath }),
  rename: (relativePath, name) => invoke('workspace_rename', { relativePath, name }),
  delete: (relativePath) => invoke('workspace_delete', { relativePath }),
  frontendReady: () => invoke('workspace_frontend_ready'),
  finishQuit: () => invoke('workspace_finish_quit'),
  registerQuitHandlers: (onBeforeQuit) =>
    Promise.all([
      getCurrentWindow().onCloseRequested((event) => {
        event.preventDefault()
        void Promise.resolve(onBeforeQuit?.()).finally(() => {
          void getCurrentWindow().destroy()
        })
      }),
      listen('workspace-quit-requested', () => {
        void Promise.resolve(onBeforeQuit?.()).finally(() => {
          void invoke('workspace_finish_quit')
        })
      }),
    ]),
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'object' && error && 'message' in error) return String(error.message)
  return String(error)
}
