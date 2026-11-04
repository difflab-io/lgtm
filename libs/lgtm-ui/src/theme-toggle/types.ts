export type ThemePreference = 'system' | 'light' | 'dark'

export interface ThemeToggleProps {
  value: ThemePreference
  size?: 'sm' | 'md'
  variant?: 'ghost' | 'outline'
  class?: string
  onchange: (value: ThemePreference) => void
}
