// Lazy, memoised Shiki highlighter. Produces per-line *inner* HTML (just the
// token spans) so the diff view keeps control of the per-line container that
// carries +/- / hunk colouring.
//
// Only a small, hard-coded set of languages is registered; adding more means
// extending `LANG_BY_EXT` and the `langs` list in the loader.

import type { Highlighter } from 'shiki'

const LANG_BY_EXT: Record<string, string> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'tsx',
  js: 'javascript',
  jsx: 'jsx',
  json: 'json',
  toml: 'toml',
  lock: 'toml', // Cargo.lock / uv.lock / poetry.lock are TOML on disk.
  yaml: 'yaml',
  yml: 'yaml',
  md: 'markdown',
  css: 'css',
  html: 'html',
  sh: 'shell',
  py: 'python',
  go: 'go',
}

const THEME = 'github-dark-default'

let singleton: Promise<Highlighter> | null = null

function loader(): Promise<Highlighter> {
  if (!singleton) {
    singleton = import('shiki').then((m) =>
      m.createHighlighter({
        themes: [THEME],
        langs: Array.from(new Set(Object.values(LANG_BY_EXT))),
      }),
    )
  }
  return singleton
}

export function langForPath(path: string): string | undefined {
  const dot = path.lastIndexOf('.')
  if (dot < 0) return undefined
  return LANG_BY_EXT[path.slice(dot + 1).toLowerCase()]
}

/**
 * Highlight a single source-code line and return the inner HTML of the first
 * (and only) rendered line — the `<span>` runs that make up the token tree.
 * Diff +/- prefix should be stripped before calling.
 */
export async function highlightLine(
  code: string,
  lang: string | undefined,
): Promise<string | null> {
  if (!lang) return null
  if (code.length === 0) return ''
  const h = await loader()
  const html = h.codeToHtml(code, { lang, theme: THEME })
  // codeToHtml wraps in <pre><code><span class="line">...</span></code></pre>.
  // Extract just the inner span runs so our own <span class="lgtm-line">
  // keeps the per-line container.
  const match = html.match(/<span class="line">([\s\S]*?)<\/span><\/code>/)
  return match ? match[1] : null
}
