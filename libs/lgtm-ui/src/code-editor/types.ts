import type { CodeEditorRootVariants } from './styles'

export interface CodeEditorProps {
  value: string
  filename?: string
  readonly?: boolean
  size?: CodeEditorRootVariants['size']
  label?: string
  class?: string
  onchange?: (value: string) => void
}
