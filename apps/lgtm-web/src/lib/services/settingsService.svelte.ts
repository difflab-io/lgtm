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
  knownRoots = $state<string[]>([])

  theme = $derived<ThemeMode>(this.snapshot?.effective.appearance?.theme ?? 'system')
  lineWrapping = $derived<LineWrap>(this.snapshot?.effective.editor?.lineWrapping ?? 'auto')
  activeRoot = $derived<string | null>(this.snapshot?.effective.workspace?.activeRoot ?? null)
  recentRoots = $derived<string[]>(
    this.knownRoots.length
      ? this.knownRoots
      : (this.snapshot?.effective.workspace?.recentRoots ?? []),
  )

  constructor(client: WorkspaceClient) {
    this.client = client
  }

  rememberRoot(root: string): void {
    if (!this.knownRoots.includes(root)) {
      this.knownRoots = [...this.knownRoots, root]
    }
  }

  async load(): Promise<ResolvedConfigSnapshot | null> {
    if (!this.client.configGet) return null
    try {
      const resolved = await this.client.configGet()
      this.applySnapshot(resolved)
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
      this.applySnapshot(updated)
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
    recentRoots: string[] = this.knownRoots,
  ): Promise<ResolvedConfigSnapshot | null> {
    this.rememberRoot(activeRoot)
    const nextRoots = recentRoots.includes(activeRoot) ? recentRoots : [...recentRoots, activeRoot]
    this.knownRoots = nextRoots
    return this.update(
      {
        workspace: {
          activeRoot,
          recentRoots: nextRoots,
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
    this.applySnapshot(snapshot)
  }

  private applySnapshot(snapshot: ResolvedConfigSnapshot): void {
    this.snapshot = snapshot
    const fromConfig = snapshot.effective.workspace?.recentRoots ?? []
    if (fromConfig.length > 0) {
      const merged = [...fromConfig]
      for (const root of this.knownRoots) {
        if (!merged.includes(root)) merged.push(root)
      }
      this.knownRoots = merged
    }
  }
}
