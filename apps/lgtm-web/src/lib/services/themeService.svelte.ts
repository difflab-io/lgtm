import type { ThemePreference } from 'lgtm-ui'

export type ResolvedThemeMode = 'light' | 'dark'

const STORAGE_KEY = 'lgtm.theme'

export class ThemeService {
  preference = $state<ThemePreference>('system')
  systemDark = $state(false)

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

    const stored = window.localStorage.getItem(STORAGE_KEY)
    if (stored === 'system' || stored === 'light' || stored === 'dark') {
      this.preference = stored
    } else {
      this.preference = 'system'
    }
    this.applyToDocument()

    const handleMediaChange = (event: MediaQueryListEvent) => {
      this.systemDark = event.matches
      this.applyToDocument()
    }

    mediaQuery.addEventListener('change', handleMediaChange)
    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange)
    }
  }

  setPreference(next: ThemePreference): void {
    this.preference = next
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(STORAGE_KEY, next)
    }
    this.applyToDocument()
  }

  // Private -------------------------------------------------------------------
  private applyToDocument(): void {
    if (typeof document === 'undefined') return
    const root = document.documentElement
    root.classList.add('theme-vercel')
    root.classList.toggle('dark', this.resolvedMode === 'dark')
    root.style.colorScheme = this.resolvedMode
  }
}
