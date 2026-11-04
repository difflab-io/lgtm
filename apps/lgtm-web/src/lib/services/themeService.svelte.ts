export type ThemeMode = 'light' | 'dark'

const STORAGE_KEY = 'lgtm.theme'

export class ThemeService {
  mode = $state<ThemeMode>('light')
  hasExplicitPreference = $state(false)

  // Derived -------------------------------------------------------------------
  readonly isDark = $derived(this.mode === 'dark')

  // Public --------------------------------------------------------------------
  init(): () => void {
    if (typeof window === 'undefined') return () => {}

    const stored = window.localStorage.getItem(STORAGE_KEY)
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')

    if (stored === 'light' || stored === 'dark') {
      this.mode = stored
      this.hasExplicitPreference = true
    } else {
      this.mode = mediaQuery.matches ? 'dark' : 'light'
      this.hasExplicitPreference = false
    }
    this.applyToDocument()

    const handleMediaChange = (event: MediaQueryListEvent) => {
      if (!this.hasExplicitPreference) {
        this.mode = event.matches ? 'dark' : 'light'
        this.applyToDocument()
      }
    }

    mediaQuery.addEventListener('change', handleMediaChange)
    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange)
    }
  }

  toggle(): void {
    this.setMode(this.mode === 'dark' ? 'light' : 'dark')
  }

  setMode(next: ThemeMode): void {
    this.mode = next
    this.hasExplicitPreference = true
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(STORAGE_KEY, next)
    }
    this.applyToDocument()
  }

  // Private -------------------------------------------------------------------
  private applyToDocument(): void {
    if (typeof document === 'undefined') return
    const root = document.documentElement
    root.classList.toggle('dark', this.mode === 'dark')
    root.style.colorScheme = this.mode
  }
}
