import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { ThemeWindowBridge } from 'lgtm-ui'
import {
  commands,
  type AppearanceSettings,
  type AppearanceSettingsPatch,
  type EditorSettings,
  type EditorSettingsPatch,
  type FileContent,
  type LineWrap,
  type LspDiagnostic,
  type LspDiagnosticsBuffer,
  type LspHoverResult,
  type LspLocation,
  type LspServerStatus,
  type LspServerStatusEvent,
  type LspWorkspaceSnapshot,
  type ResolvedConfigSnapshot,
  type ThemeMode,
  type UserSettings,
  type UserSettingsPatch,
  type WorkspaceError,
  type WorkspaceListing,
  type WorkspaceSettings,
  type WorkspaceSettingsPatch,
} from '../gen/bindings'

export type {
  AppearanceSettings,
  AppearanceSettingsPatch,
  EditorSettings,
  EditorSettingsPatch,
  FileContent,
  LineWrap,
  LspDiagnostic,
  LspDiagnosticsBuffer,
  LspHoverResult,
  LspLocation,
  LspServerStatus,
  LspServerStatusEvent,
  LspWorkspaceSnapshot,
  ResolvedConfigSnapshot,
  ThemeMode,
  UserSettings,
  UserSettingsPatch,
  WorkspaceError,
  WorkspaceListing,
  WorkspaceSettings,
  WorkspaceSettingsPatch,
}

export interface WorkspaceNode {
  path: string
  name: string
  kind: 'directory' | 'file' | 'unavailable'
}

export interface LspFileDiagnosticsEvent {
  workspaceRoot: string
  path: string
  languageId: string
  diagnostics: LspDiagnostic[]
}

export interface WorkspaceClient {
  isDesktop(): boolean
  themeWindowBridge?(): ThemeWindowBridge | undefined
  open(path?: string): Promise<{ root: string; entries: WorkspaceNode[] }>
  pickFolder(): Promise<string | null>
  list(path: string): Promise<WorkspaceNode[]>
  read(path: string): Promise<FileContent>
  openGlobalConfig?(): Promise<FileContent>
  openLocalConfig?(): Promise<FileContent>
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
  configGet?(): Promise<ResolvedConfigSnapshot>
  registerConfigHandlers?(handlers: {
    onConfigUpdated: (snapshot: ResolvedConfigSnapshot) => void
  }): Promise<(() => void)[]>
  registerLspHandlers(handlers: {
    onStatus: (event: LspServerStatusEvent) => void
    onDiagnostics: (event: LspFileDiagnosticsEvent) => void
  }): Promise<(() => void)[]>
}

export const workspaceClient: WorkspaceClient = {
  isDesktop: () => isTauri(),
  themeWindowBridge: () => {
    if (!isTauri()) return undefined
    const win = getCurrentWindow()
    return {
      setTheme: (mode) => win.setTheme(mode),
      getTheme: async () => {
        const theme = await win.theme()
        return theme === 'dark' || theme === 'light' ? theme : null
      },
      onThemeChanged: (handler) =>
        win.onThemeChanged(({ payload }) => {
          if (payload === 'dark' || payload === 'light') {
            handler(payload)
          }
        }),
    }
  },
  open: (path?: string) => {
    if (!isTauri())
      return Promise.reject(new Error('Open LGTM in the desktop app to access a local workspace.'))
    return invoke('workspace_open', path ? { path } : {})
  },
  pickFolder: () => invoke('workspace_pick_folder'),
  list: (relativePath) => invoke('workspace_list', { relativePath }),
  read: (relativePath) => invoke('workspace_read', { relativePath }),
  openGlobalConfig: () => commands.workspaceOpenGlobalConfig(),
  openLocalConfig: () => commands.workspaceOpenLocalConfig(),
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
  configGet: () => commands.configGet(),
  registerConfigHandlers: (handlers) =>
    Promise.all([
      listen<ResolvedConfigSnapshot>('config-updated', (event) => {
        handlers.onConfigUpdated(event.payload)
      }),
    ]),
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

/** Converts an unknown error value into a displayable message string. */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'object' && error && 'message' in error) return String(error.message)
  return String(error)
}
