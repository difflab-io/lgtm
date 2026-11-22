import type { TerminalTheme } from './types'

/** Monospace font stack prioritizing Nerd Font glyphs for TUIs (`yazi`, `starship`, `nvim`). */
export const TERMINAL_FONT_FAMILY =
  '"MesloLGS Nerd Font Mono", "CaskaydiaCove Nerd Font Mono", "ZedMono Nerd Font Mono", "IosevkaTerm Nerd Font Mono", "FiraMono Nerd Font Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace'

/** Reads LGTM's active light/dark CSS tokens and returns a `@wterm/ghostty` palette. */
export function getTerminalTheme(): TerminalTheme {
  const isDark =
    typeof document !== 'undefined' && document.documentElement.classList.contains('dark')
  if (isDark) {
    return {
      background: '#0a0a0a',
      foreground: '#ededed',
      cursor: '#ededed',
      selectionBackground: '#262626',
      black: '#171717',
      red: '#f75f8f',
      green: '#62c073',
      yellow: '#ff990a',
      blue: '#52a8ff',
      magenta: '#bf7af0',
      cyan: '#56d4dd',
      white: '#ededed',
      brightBlack: '#737373',
      brightRed: '#ff6b9a',
      brightGreen: '#79d98a',
      brightYellow: '#ffb547',
      brightBlue: '#79b8ff',
      brightMagenta: '#d49eff',
      brightCyan: '#7ee6ed',
      brightWhite: '#ffffff',
    }
  }
  return {
    background: '#fafafa',
    foreground: '#171717',
    cursor: '#171717',
    selectionBackground: '#e5e5e5',
    black: '#171717',
    red: '#d92d57',
    green: '#1e8e3e',
    yellow: '#b25e09',
    blue: '#0068d6',
    magenta: '#7c3aed',
    cyan: '#0e7490',
    white: '#d4d4d4',
    brightBlack: '#525252',
    brightRed: '#e11d48',
    brightGreen: '#16a34a',
    brightYellow: '#ca8a04',
    brightBlue: '#2563eb',
    brightMagenta: '#9333ea',
    brightCyan: '#0891b2',
    brightWhite: '#ffffff',
  }
}

/** Observes `document.documentElement` class mutations to react to LGTM light/dark theme switches. */
export function observeTerminalTheme(onChange: (theme: TerminalTheme) => void): () => void {
  if (typeof MutationObserver === 'undefined' || typeof document === 'undefined') {
    return () => {}
  }
  const observer = new MutationObserver(() => {
    onChange(getTerminalTheme())
  })
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  })
  return () => observer.disconnect()
}

/** Converts a hex `#rrggbb` color string into a 24-bit integer for `@wterm/dom`. */
export function hexToRgbInt(hex: string): number {
  return Number.parseInt(hex.replace('#', ''), 16) || 0
}

/** Converts a [`TerminalTheme`] into `@wterm/dom`'s `setThemeColors` payload. */
export function toWtermThemeColors(t: TerminalTheme) {
  return {
    foreground: hexToRgbInt(t.foreground),
    background: hexToRgbInt(t.background),
    cursor: hexToRgbInt(t.cursor),
    palette: [
      hexToRgbInt(t.black),
      hexToRgbInt(t.red),
      hexToRgbInt(t.green),
      hexToRgbInt(t.yellow),
      hexToRgbInt(t.blue),
      hexToRgbInt(t.magenta),
      hexToRgbInt(t.cyan),
      hexToRgbInt(t.white),
      hexToRgbInt(t.brightBlack),
      hexToRgbInt(t.brightRed),
      hexToRgbInt(t.brightGreen),
      hexToRgbInt(t.brightYellow),
      hexToRgbInt(t.brightBlue),
      hexToRgbInt(t.brightMagenta),
      hexToRgbInt(t.brightCyan),
      hexToRgbInt(t.brightWhite),
    ],
  }
}
