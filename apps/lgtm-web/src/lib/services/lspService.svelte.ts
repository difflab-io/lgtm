import type { CodeEditorHoverResult, MenuItemIndicatorTone } from 'lgtm-ui'
import type {
  LspDiagnostic,
  LspDiagnosticsBuffer,
  LspFileDiagnosticsEvent,
  LspLocation,
  LspServerStatusEvent,
  WorkspaceClient,
} from '../clients/workspaceClient'

export interface LspDiagnosticEntry {
  path: string
  from: number
  line: number
  character: number
  severity: LspDiagnostic['severity']
  message: string
}

/** Reactive Svelte 5 service managing workspace LSP server status, diagnostics, hover, and navigation. */
export class LspService {
  servers = $state<Record<string, LspServerStatusEvent>>({})
  diagnostics = $state<Record<string, LspDiagnostic[]>>({})

  /** Constructs an `LspService` backed by `client`. */
  constructor(private client: WorkspaceClient) {}

  /** Returns all tracked language server status events for the active workspace. */
  get allServers(): LspServerStatusEvent[] {
    return Object.values(this.servers)
  }

  /** Returns flattened diagnostic entries across all workspace files. */
  get diagnosticEntries(): LspDiagnosticEntry[] {
    const items: LspDiagnosticEntry[] = []
    for (const [path, list] of Object.entries(this.diagnostics)) {
      for (const diag of list) {
        items.push({
          path,
          from: diag.from,
          line: diag.line,
          character: diag.character,
          severity: diag.severity,
          message: diag.message,
        })
      }
    }
    return items
  }

  /** Clears all cached server statuses and file diagnostics when switching workspaces. */
  reset() {
    this.servers = {}
    this.diagnostics = {}
  }

  /** Returns the diagnostics list for a workspace-relative or config file path. */
  getDiagnosticsForPath(path?: string | null): LspDiagnostic[] {
    if (!path) return []
    if (this.diagnostics[path]) return this.diagnostics[path]
    if (path === '~/.lgtm/config.json') {
      for (const [key, list] of Object.entries(this.diagnostics)) {
        if (
          key !== '.lgtm/config.json' &&
          (key.endsWith('/.lgtm/config.json') || key.endsWith('\\.lgtm\\config.json'))
        ) {
          return list
        }
      }
    }
    return []
  }

  /** Returns the language server status matching `activeLanguageId` or the first active server. */
  getActiveServer(activeLanguageId: string | null): LspServerStatusEvent | null {
    if (activeLanguageId && this.servers[activeLanguageId]) {
      return this.servers[activeLanguageId]
    }
    return this.allServers[0] ?? null
  }

  /** Computes the status indicator tone and summary label for a language server menu item. */
  getServerIndicator(
    server: LspServerStatusEvent,
    activeLanguageId: string | null,
  ): { tone: MenuItemIndicatorTone; label: string } {
    if (server.status === 'unavailable' || server.status === 'starting') {
      return {
        tone: 'inactive',
        label: server.status === 'starting' ? 'Starting' : 'Inactive',
      }
    }
    if (server.status === 'error' || server.errorCount > 0) {
      return {
        tone: 'error',
        label: `${server.errorCount} ${server.errorCount === 1 ? 'error' : 'errors'}`,
      }
    }
    if (server.warningCount > 0) {
      return {
        tone: 'warning',
        label: `${server.warningCount} ${server.warningCount === 1 ? 'warning' : 'warnings'}`,
      }
    }
    if (activeLanguageId && server.languageId === activeLanguageId) {
      return {
        tone: 'active-page',
        label: 'Active for page',
      }
    }
    return {
      tone: 'active-project',
      label: 'Active for project',
    }
  }

  /** Records initial language server status attached to a newly read file. */
  recordFileStatus(status?: LspServerStatusEvent | null) {
    if (status && !this.servers[status.languageId]) {
      this.servers = {
        ...this.servers,
        [status.languageId]: status,
      }
    }
  }

  /** Applies an incoming `lsp-status` event to reactive state. */
  handleStatus(event: LspServerStatusEvent) {
    this.servers = {
      ...this.servers,
      [event.languageId]: event,
    }
  }

  /** Applies an incoming `lsp-diagnostics` event to reactive state. */
  handleDiagnostics(event: LspFileDiagnosticsEvent) {
    this.diagnostics = {
      ...this.diagnostics,
      [event.path]: event.diagnostics,
    }
  }

  /** Pulls the current workspace LSP server and diagnostics snapshot from the backend. */
  async syncState(): Promise<void> {
    try {
      const snapshot = await this.client.lspGetState()
      const nextServers: Record<string, LspServerStatusEvent> = { ...this.servers }
      for (const srv of snapshot.servers) {
        nextServers[srv.languageId] = srv
      }
      if (JSON.stringify(nextServers) !== JSON.stringify(this.servers)) {
        this.servers = nextServers
      }
      const nextDiagnostics = {
        ...this.diagnostics,
        ...snapshot.diagnosticsByPath,
      }
      if (JSON.stringify(nextDiagnostics) !== JSON.stringify(this.diagnostics)) {
        this.diagnostics = nextDiagnostics
      }
    } catch {
      // Non-LSP test fixtures or browser-only mode may not implement lsp_get_state.
    }
  }

  /** Sends an in-memory `textDocument/didChange` notification to the language server. */
  notifyDidChange(path: string, content: string) {
    void this.client.lspDidChange(path, content).catch(() => {})
  }

  /** Queries `textDocument/hover` documentation at a UTF-16 editor offset. */
  async requestHover(path: string, offset: number): Promise<CodeEditorHoverResult | null> {
    try {
      const result = await this.client.lspHover(path, offset)
      if (!result || !result.contents.trim()) return null
      return {
        contents: result.contents,
        ...(result.from !== undefined && result.from !== null ? { from: result.from } : {}),
        ...(result.to !== undefined && result.to !== null ? { to: result.to } : {}),
      }
    } catch {
      return null
    }
  }

  /** Queries `textDocument/definition` locations at a UTF-16 editor offset. */
  async getDefinitions(path: string, offset: number): Promise<LspLocation[]> {
    try {
      return await this.client.lspGotoDefinition(path, offset)
    } catch {
      return []
    }
  }

  /** Fetches the formatted plain-text diagnostics buffer for all servers or `languageId`. */
  async getDiagnosticsBuffer(languageId?: string): Promise<LspDiagnosticsBuffer> {
    return this.client.lspGetDiagnosticsBuffer(languageId)
  }

  /** Restarts the language server for `languageId` in the active workspace. */
  async restartServer(languageId: string): Promise<void> {
    try {
      const status = await this.client.lspRestart(languageId)
      this.handleStatus(status)
    } catch {
      await this.syncState()
    }
  }
}
