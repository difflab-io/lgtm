import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'

export interface WorkspaceNode {
  path: string
  name: string
  kind: 'directory' | 'file' | 'unavailable'
}

export type LspServerStatus = 'starting' | 'ready' | 'unavailable' | 'error'

export interface LspServerStatusEvent {
  workspaceRoot: string
  languageId: string
  projectRoot: string
  status: LspServerStatus
  command: string
  message?: string | null
  errorCount: number
  warningCount: number
}

export interface LspDiagnostic {
  from: number
  to: number
  line: number
  character: number
  endLine: number
  endCharacter: number
  severity: 'error' | 'warning' | 'info' | 'hint'
  message: string
  source?: string | null
  code?: string | null
}

export interface LspFileDiagnosticsEvent {
  workspaceRoot: string
  path: string
  languageId: string
  diagnostics: LspDiagnostic[]
}

export interface LspHoverResult {
  contents: string
  from?: number | null
  to?: number | null
}

export interface LspLocation {
  path: string
  offset: number
  endOffset: number
  line: number
  character: number
}

export interface LspWorkspaceSnapshot {
  workspaceRoot: string
  servers: LspServerStatusEvent[]
  diagnosticsByPath: Record<string, LspDiagnostic[]>
}

export interface LspDiagnosticsBuffer {
  title: string
  languageId?: string | null
  content: string
}

export interface FileContent {
  content: string
  revision: string
  languageId?: string | null
  lspStatus?: LspServerStatusEvent | null
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
  lspDidChange(path: string, content: string): Promise<void>
  lspHover(path: string, offset: number): Promise<LspHoverResult | null>
  lspGotoDefinition(path: string, offset: number): Promise<LspLocation[]>
  lspGetState(): Promise<LspWorkspaceSnapshot>
  lspGetDiagnosticsBuffer(languageId?: string): Promise<LspDiagnosticsBuffer>
  lspRestart(languageId: string): Promise<LspServerStatusEvent>
  registerLspHandlers(handlers: {
    onStatus: (event: LspServerStatusEvent) => void
    onDiagnostics: (event: LspFileDiagnosticsEvent) => void
  }): Promise<(() => void)[]>
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
  lspDidChange: (relativePath, content) => invoke('lsp_did_change', { relativePath, content }),
  lspHover: (relativePath, offset) => invoke('lsp_hover', { relativePath, offset }),
  lspGotoDefinition: (relativePath, offset) =>
    invoke('lsp_goto_definition', { relativePath, offset }),
  lspGetState: () => invoke('lsp_get_state'),
  lspGetDiagnosticsBuffer: (languageId?: string) =>
    invoke('lsp_get_diagnostics_buffer', languageId ? { languageId } : {}),
  lspRestart: (languageId: string) => invoke('lsp_restart', { languageId }),
  registerLspHandlers: (handlers) =>
    Promise.all([
      listen<LspServerStatusEvent>('lsp-status', (event) => {
        handlers.onStatus(event.payload)
      }),
      listen<LspFileDiagnosticsEvent>('lsp-diagnostics', (event) => {
        handlers.onDiagnostics(event.payload)
      }),
    ]),
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'object' && error && 'message' in error) return String(error.message)
  return String(error)
}
