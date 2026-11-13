import { markdown } from '@codemirror/lang-markdown'
import { LanguageDescription, type LanguageSupport } from '@codemirror/language'
import { languages } from '@codemirror/language-data'
import type { Extension } from '@codemirror/state'

/**
 * `@codemirror/language-data` ships lazy language descriptors whose `load()` method returns a
 * `Promise<LanguageSupport>`. If an `EditorState` is created before the language grammar resolves,
 * the initial render shows unhighlighted text until the async import completes.
 *
 * Prewarming common source file extensions at module initialization populates `loadedLanguageSupport`
 * so `getCachedLanguageExtension` can attach the grammar synchronously during `onMount`, allowing
 * `ensureSyntaxTree(state, state.doc.length, 100)` to parse and highlight the viewport on the first frame.
 */
const PREWARMED_SAMPLE_FILENAMES = [
  'file.rs',
  'file.ts',
  'file.tsx',
  'file.js',
  'file.json',
  'file.toml',
  'file.html',
  'file.css',
  'file.py',
  'file.yaml',
  'file.sh',
] as const

const loadedLanguageSupport = new Map<LanguageDescription, LanguageSupport>()

/** Preloads common CodeMirror language grammars into `loadedLanguageSupport` in the background. */
export function preloadCommonLanguages(): void {
  for (const sample of PREWARMED_SAMPLE_FILENAMES) {
    const desc = LanguageDescription.matchFilename(languages, sample)
    if (desc && !desc.support && !loadedLanguageSupport.has(desc)) {
      void desc.load().then((support) => {
        loadedLanguageSupport.set(desc, support)
      })
    }
  }
}

preloadCommonLanguages()

function isMarkdownFilename(targetName: string): boolean {
  const lower = targetName.toLowerCase()
  return (
    lower.endsWith('.md') ||
    lower.endsWith('.markdown') ||
    lower.endsWith('.mdx') ||
    lower.endsWith('.mkd')
  )
}

/**
 * Returns a synchronous CodeMirror language `Extension` when the grammar for `targetName` is
 * already cached (or when `targetName` has no matching language), or `null` if the grammar must be
 * loaded asynchronously via `loadLanguageExtension`.
 */
export function getCachedLanguageExtension(targetName: string | undefined): Extension | null {
  if (!targetName) return []
  if (isMarkdownFilename(targetName)) {
    return markdown({ codeLanguages: languages })
  }
  const description = LanguageDescription.matchFilename(languages, targetName)
  if (!description) return []
  return description.support ?? loadedLanguageSupport.get(description) ?? null
}

/** Asynchronously loads and caches the CodeMirror language `Extension` for `targetName`. */
export async function loadLanguageExtension(targetName: string | undefined): Promise<Extension> {
  const cached = getCachedLanguageExtension(targetName)
  if (cached !== null) return cached
  const description = targetName ? LanguageDescription.matchFilename(languages, targetName) : null
  if (!description) return []
  const support = await description.load()
  loadedLanguageSupport.set(description, support)
  return support
}
