export { toCodeMirrorDiagnostics } from './lspExtension'
export { codeEditorHighlightStyle, toLineWrappingExtension } from './themeExtension'

/** Returns whether `targetName` has a Markdown file extension. */
export function isMarkdownTarget(targetName: string): boolean {
  const lower = targetName.toLowerCase()
  return (
    lower.endsWith('.md') ||
    lower.endsWith('.markdown') ||
    lower.endsWith('.mdx') ||
    lower.endsWith('.mkd')
  )
}
