export { default as DiffEditor } from './DiffEditor.svelte'
export { goToNextHunk, goToPrevHunk } from '../code-editor/modalKeymap'
export {
  diffEditorRootVariants,
  diffEditorStyles,
  type DiffEditorRootVariants,
  type DiffEditorVariantProps,
} from './styles'
export type { DiffEditorMode, DiffEditorProps } from './types'
