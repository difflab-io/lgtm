import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { ThemePreference } from 'lgtm-ui'

export type ResolvedThemeMode = 'light' | 'dark'

const STORAGE_KEY = 'lgtm.theme'

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
    this.applyThemePreference(this.preference)

    const handleMediaChange = (event: MediaQueryListEvent) => {
      this.systemDark = event.matches
      this.applyToDocument()
    }

    mediaQuery.addEventListener('change', handleMediaChange)

    let unlistenTauriTheme: (() => void) | undefined
    if (isTauri()) {
      void getCurrentWindow()
        .onThemeChanged(({ payload }) => {
          this.systemDark = payload === 'dark'
          this.applyToDocument()
        })
        .then((unlisten) => {
          unlistenTauriTheme = unlisten
        })
        .catch(() => {})
    }

    return () => {
      mediaQuery.removeEventListener('change', handleMediaChange)
      unlistenTauriTheme?.()
    }
  }

  setPreference(next: ThemePreference): void {
    this.preference = next
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(STORAGE_KEY, next)
    }
    this.applyThemePreference(next)
  }

  // Private -------------------------------------------------------------------
  private applyThemePreference(mode: ThemePreference): void {
    this.applyToDocument()
    if (!isTauri()) return
    const win = getCurrentWindow()
    void win
      .setTheme(mode === 'system' ? null : mode)
      .catch(() => {})
      .then(() => win.theme())
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
    root.classList.remove('theme-vercel')
    root.classList.add('theme-lgtm')
    root.classList.toggle('dark', this.resolvedMode === 'dark')
    root.style.colorScheme = this.resolvedMode
  }
}
