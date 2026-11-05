export type ThemePreference = 'system' | 'light' | 'dark'
export type ResolvedThemeMode = 'light' | 'dark'

export interface ThemeWindowBridge {
  setTheme(mode: ResolvedThemeMode | null): Promise<void>
  getTheme(): Promise<ResolvedThemeMode | null>
  onThemeChanged(handler: (mode: ResolvedThemeMode) => void): Promise<() => void>
}

export interface ThemeServiceOptions {
  storageKey?: string
  themeClass?: string
  windowBridge?: ThemeWindowBridge
}

export interface ThemeToggleProps {
  value: ThemePreference
  size?: 'sm' | 'md'
  variant?: 'ghost' | 'outline'
  class?: string
  onchange: (value: ThemePreference) => void
}
