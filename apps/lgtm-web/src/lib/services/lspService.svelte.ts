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

  constructor(private client: WorkspaceClient) {}

  get allServers(): LspServerStatusEvent[] {
    return Object.values(this.servers)
  }

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

  reset() {
    this.servers = {}
    this.diagnostics = {}
  }

  getDiagnosticsForPath(path?: string | null): LspDiagnostic[] {
    if (!path) return []
    return this.diagnostics[path] ?? []
  }

  getActiveServer(activeLanguageId: string | null): LspServerStatusEvent | null {
    if (activeLanguageId && this.servers[activeLanguageId]) {
      return this.servers[activeLanguageId]
    }
    return this.allServers[0] ?? null
  }

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

  recordFileStatus(status?: LspServerStatusEvent | null) {
    if (status && !this.servers[status.languageId]) {
      this.servers = {
        ...this.servers,
        [status.languageId]: status,
      }
    }
  }

  handleStatus(event: LspServerStatusEvent) {
    this.servers = {
      ...this.servers,
      [event.languageId]: event,
    }
  }

  handleDiagnostics(event: LspFileDiagnosticsEvent) {
    this.diagnostics = {
      ...this.diagnostics,
      [event.path]: event.diagnostics,
    }
  }

  async syncState(): Promise<void> {
    try {
      const snapshot = await this.client.lspGetState()
      const nextServers: Record<string, LspServerStatusEvent> = { ...this.servers }
      for (const srv of snapshot.servers) {
        nextServers[srv.languageId] = srv
      }
      this.servers = nextServers
      this.diagnostics = {
        ...this.diagnostics,
        ...snapshot.diagnosticsByPath,
      }
    } catch {
      // Non-LSP test fixtures or browser-only mode may not implement lsp_get_state.
    }
  }

  notifyDidChange(path: string, content: string) {
    void this.client.lspDidChange(path, content).catch(() => {})
  }

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

  async getDefinitions(path: string, offset: number): Promise<LspLocation[]> {
    try {
      return await this.client.lspGotoDefinition(path, offset)
    } catch {
      return []
    }
  }

  async getDiagnosticsBuffer(languageId?: string): Promise<LspDiagnosticsBuffer> {
    return this.client.lspGetDiagnosticsBuffer(languageId)
  }

  async restartServer(languageId: string): Promise<void> {
    try {
      const status = await this.client.lspRestart(languageId)
      this.handleStatus(status)
    } catch {
      await this.syncState()
    }
  }
}
