/** Palette and surface colors applied to `@wterm/ghostty`. */
export interface TerminalTheme {
  background: string
  foreground: string
  cursor: string
  selectionBackground: string
  black: string
  red: string
  green: string
  yellow: string
  blue: string
  magenta: string
  cyan: string
  white: string
  brightBlack: string
  brightRed: string
  brightGreen: string
  brightYellow: string
  brightBlue: string
  brightMagenta: string
  brightCyan: string
  brightWhite: string
}

/** Imperative handle exposed by `TerminalSurface` for streaming bytes and focus/fit control. */
export interface TerminalSurfaceHandle {
  /** Writes raw UTF-8/ANSI/Kitty bytes into `@wterm/ghostty`. */
  write: (bytes: Uint8Array) => void
  /** Clears the visible screen buffer. */
  clear: () => void
  /** Focuses the terminal's internal editable `<textarea>`. */
  focus: () => void
  /** Refits the terminal grid to its container dimensions. */
  fit: () => void
}

/** Props accepted by `TerminalSurface.svelte`. */
export interface TerminalSurfaceProps {
  /** Active `herdr-core` session identifier bound to this surface. */
  sessionId: string
  /** Optional custom theme override; defaults to LGTM CSS variables. */
  theme?: TerminalTheme
  /** Extra Tailwind classes applied to the viewport root. */
  class?: string
  /** Callback invoked when the user types or pastes bytes into the terminal. */
  onInput: (bytes: Uint8Array) => void
  /** Callback invoked when the fitted terminal grid changes dimensions. */
  onResize: (cols: number, rows: number) => void
  /** Callback invoked once the `@wterm/ghostty` surface handle is ready. */
  onReady?: (handle: TerminalSurfaceHandle) => void
}
