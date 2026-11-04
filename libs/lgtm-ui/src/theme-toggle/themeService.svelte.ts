import type {
  ResolvedThemeMode,
  ThemePreference,
  ThemeServiceOptions,
  ThemeWindowBridge,
} from './types'

const DEFAULT_STORAGE_KEY = 'helical.theme'

function readSystemDarkMode(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false
  try {
    return window.matchMedia('(prefers-color-scheme: dark)').matches
  } catch {
    return false
  }
}

export class ThemeService {
  preference = $state<ThemePreference>('system')
  systemDark = $state(readSystemDarkMode())

  private readonly storageKey: string
  private readonly themeClass: string | undefined
  private readonly windowBridge: ThemeWindowBridge | undefined

  constructor(options: ThemeServiceOptions = {}) {
    this.storageKey = options.storageKey ?? DEFAULT_STORAGE_KEY
    this.themeClass = options.themeClass
    this.windowBridge = options.windowBridge
  }

  // Derived -------------------------------------------------------------------
  readonly resolvedMode = $derived<ResolvedThemeMode>(
    this.preference === 'system' ? (this.systemDark ? 'dark' : 'light') : this.preference,
  )
  readonly isDark = $derived(this.resolvedMode === 'dark')

  // Public --------------------------------------------------------------------
  init(): () => void {
    if (typeof window === 'undefined') return () => {}

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    this.systemDark = mediaQuery.matches

    const stored = window.localStorage.getItem(this.storageKey)
    if (stored === 'system' || stored === 'light' || stored === 'dark') {
      this.preference = stored
    } else {
      this.preference = 'system'
    }
    this.applyThemePreference(this.preference)

    const handleMediaChange = (event: MediaQueryListEvent) => {
      this.systemDark = event.matches
      this.applyToDocument()
    }

    mediaQuery.addEventListener('change', handleMediaChange)

    let unlistenWindowTheme: (() => void) | undefined
    if (this.windowBridge) {
      void this.windowBridge
        .onThemeChanged((mode) => {
          this.systemDark = mode === 'dark'
          this.applyToDocument()
        })
        .then((unlisten) => {
          unlistenWindowTheme = unlisten
        })
        .catch(() => {})
    }

    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange)
      unlistenWindowTheme?.()
    }
  }

  setPreference(next: ThemePreference): void {
    this.preference = next
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(this.storageKey, next)
    }
    this.applyThemePreference(next)
  }

  // Private -------------------------------------------------------------------
  private applyThemePreference(mode: ThemePreference): void {
    this.applyToDocument()
    if (!this.windowBridge) return
    const bridge = this.windowBridge
    void bridge
      .setTheme(mode === 'system' ? null : mode)
      .catch(() => {})
      .then(() => bridge.getTheme())
      .then((osTheme) => {
        if (osTheme === 'dark' || osTheme === 'light') {
          this.systemDark = osTheme === 'dark'
        } else {
          this.systemDark = readSystemDarkMode()
        }
        this.applyToDocument()
      })
      .catch(() => {
        this.systemDark = readSystemDarkMode()
        this.applyToDocument()
      })
  }

  private applyToDocument(): void {
    if (typeof document === 'undefined') return
    const root = document.documentElement
    if (this.themeClass) {
      root.classList.add(this.themeClass)
    }
    root.classList.toggle('dark', this.resolvedMode === 'dark')
    root.style.colorScheme = this.resolvedMode
  }
}
