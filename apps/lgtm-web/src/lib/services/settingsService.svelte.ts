import {
  type ConfigDocument,
  type ConfigFormat,
  type ConfigScope,
  type LineWrap,
  type ResolvedConfigSnapshot,
  type ThemeMode,
  type UserSettingsPatch,
  type WorkspaceClient,
} from '../clients/workspaceClient'

export class SettingsService {
  private readonly client: WorkspaceClient

  snapshot = $state<ResolvedConfigSnapshot | null>(null)

  theme = $derived<ThemeMode>(this.snapshot?.effective.appearance?.theme ?? 'system')
  lineWrapping = $derived<LineWrap>(this.snapshot?.effective.editor?.lineWrapping ?? 'auto')
  activeRoot = $derived<string | null>(this.snapshot?.effective.workspace?.activeRoot ?? null)
  recentRoots = $derived<string[]>(this.snapshot?.effective.workspace?.recentRoots ?? [])

  constructor(client: WorkspaceClient) {
    this.client = client
  }

  async load(): Promise<ResolvedConfigSnapshot | null> {
    if (!this.client.configGet) return null
    try {
      const resolved = await this.client.configGet()
      this.snapshot = resolved
      return resolved
    } catch {
      return null
    }
  }

  async update(
    patch: UserSettingsPatch,
    scope: ConfigScope | null = null,
  ): Promise<ResolvedConfigSnapshot | null> {
    if (!this.client.configUpdate) return null
    try {
      const updated = await this.client.configUpdate(patch, scope)
      this.snapshot = updated
      return updated
    } catch {
      return null
    }
  }

  async setTheme(
    theme: ThemeMode,
    scope: ConfigScope | null = null,
  ): Promise<ResolvedConfigSnapshot | null> {
    return this.update({ appearance: { theme } }, scope)
  }

  async setLineWrapping(
    lineWrapping: LineWrap,
    scope: ConfigScope | null = null,
  ): Promise<ResolvedConfigSnapshot | null> {
    return this.update({ editor: { lineWrapping } }, scope)
  }

  async setWorkspaceRoots(
    activeRoot: string,
    recentRoots: string[],
  ): Promise<ResolvedConfigSnapshot | null> {
    return this.update(
      {
        workspace: {
          activeRoot,
          recentRoots,
        },
      },
      'global',
    )
  }

  async openDocument(
    scope: ConfigScope = 'global',
    format: ConfigFormat | null = null,
  ): Promise<ConfigDocument | null> {
    if (!this.client.configOpenDocument) return null
    return this.client.configOpenDocument(scope, format)
  }

  async writeDocument(
    scope: ConfigScope,
    content: string,
    format: ConfigFormat | null = null,
  ): Promise<ConfigDocument | null> {
    if (!this.client.configWriteDocument) return null
    return this.client.configWriteDocument(scope, content, format)
  }

  handleUpdated(snapshot: ResolvedConfigSnapshot): void {
    this.snapshot = snapshot
  }
}
