import {
  type CommandStyle,
  type ConfigScope,
  type KeybindPreset,
  type LgtmConfigPatch,
  type LineWrap,
  type ResolvedConfigSnapshot,
  type ThemeMode,
  type WorkspaceClient,
} from '../clients/workspaceClient'

/**
 * Reactive Svelte 5 service exposing resolved user preferences loaded from `~/.lgtm/config.json` and `<workspace>/.lgtm/config.json`.
 */
export class SettingsService {
  private readonly client: WorkspaceClient

  /** Latest resolved configuration snapshot from the desktop backend. */
  snapshot = $state<ResolvedConfigSnapshot | null>(null)
  /** Session-tracked workspace roots combined with `workspace.recentRoots` from `config.json`. */
  knownRoots = $state<string[]>([])

  /** Effective color theme preference (`system`, `light`, or `dark`). */
  theme = $derived<ThemeMode>(this.snapshot?.effective.appearance?.theme ?? 'system')
  /** Effective editor line-wrapping mode (`off`, `auto`, or a character column width). */
  lineWrapping = $derived<LineWrap>(this.snapshot?.effective.editor?.lineWrapping ?? 'auto')
  /** Effective command surface style (`menu` or `palette`). */
  commandStyle = $derived<CommandStyle>(
    this.snapshot?.effective.keybindings?.commandStyle ?? 'menu',
  )
  /** Effective modal keybinding preset (`helix`, `vim`, or `kakoune`). */
  keybindPreset = $derived<KeybindPreset>(this.snapshot?.effective.keybindings?.preset ?? 'helix')
  /** Whether WhichKey popup only opens for leader (`Space`) sequences. */
  leaderOnly = $derived<boolean>(this.snapshot?.effective.keybindings?.leaderOnly ?? false)
  /** Whether the active spatial pane renders a focus outline. */
  paneOutline = $derived<boolean>(this.snapshot?.effective.keybindings?.paneOutline ?? true)
  /** Whether clicking the code editor switches from Normal to Insert mode. */
  clickToInsert = $derived<boolean>(this.snapshot?.effective.keybindings?.clickToInsert ?? true)
  /** Whether workspace LSP integration is enabled. */
  lspEnabled = $derived<boolean>(this.snapshot?.effective.lsp?.enabled ?? true)
  /** Effective configured active workspace root path, if set in `config.json`. */
  activeRoot = $derived<string | null>(this.snapshot?.effective.workspace?.activeRoot ?? null)
  /** Effective list of recent workspace root paths for the workspace selector dropdown. */
  recentRoots = $derived<string[]>(
    this.knownRoots.length
      ? this.knownRoots
      : (this.snapshot?.effective.workspace?.recentRoots ?? []),
  )

  /** Constructs a `SettingsService` backed by `client`. */
  constructor(client: WorkspaceClient) {
    this.client = client
  }

  // Public --------------------------------------------------------------------

  /** Records an opened workspace root in the session's known roots list. */
  rememberRoot(root: string): void {
    if (!this.knownRoots.includes(root)) {
      this.knownRoots = [...this.knownRoots, root]
    }
  }

  /** Loads the resolved configuration snapshot from `config_get` and updates reactive state. */
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

  /** Persists a partial configuration patch via `config_update` and updates reactive state. */
  async update(
    patch: LgtmConfigPatch,
    scope?: ConfigScope | null,
  ): Promise<ResolvedConfigSnapshot | null> {
    if (!this.client.configUpdate) return null
    try {
      const resolved = await this.client.configUpdate(patch, scope ?? null)
      this.applySnapshot(resolved)
      return resolved
    } catch {
      return null
    }
  }

  /** Applies a live `config-updated` snapshot emitted after saving a `config.json` file. */
  handleUpdated(snapshot: ResolvedConfigSnapshot): void {
    this.applySnapshot(snapshot)
  }

  // Private -------------------------------------------------------------------

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
