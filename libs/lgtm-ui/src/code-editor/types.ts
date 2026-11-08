import type { EditorView } from '@codemirror/view'
import type { CodeEditorRootVariants } from './styles'

export type { EditorView }

export type CodeEditorDiagnosticSeverity = 'error' | 'warning' | 'info' | 'hint'

export type CodeEditorLineWrapping = boolean | 'off' | 'auto' | number

export interface CodeEditorDiagnostic {
  from: number
  to: number
  severity: CodeEditorDiagnosticSeverity
  message: string
  source?: string
  code?: string
}

export interface CodeEditorHoverResult {
  contents: string
  from?: number
  to?: number
}

export interface CodeEditorCompletionItem {
  label: string
  kind?: string
  detail?: string
  documentation?: string
  insertText?: string
  from?: number
  to?: number
}

export interface CodeEditorSignatureInformation {
  label: string
  documentation?: string
  parameters: string[]
}

export interface CodeEditorSignatureHelp {
  signatures: CodeEditorSignatureInformation[]
  activeSignature: number
  activeParameter: number
}

export interface CodeEditorProps {
  value: string
  filename?: string
  readonly?: boolean
  lineWrapping?: CodeEditorLineWrapping
  clickToInsert?: boolean
  size?: CodeEditorRootVariants['size']
  label?: string
  class?: string
  diagnostics?: CodeEditorDiagnostic[]
  selectionOffset?: number | null
  onchange?: (value: string) => void
  onhover?: (offset: number) => Promise<CodeEditorHoverResult | null>
  ongotodefinition?: (offset: number) => void | Promise<void>
  oncompletion?: (offset: number) => Promise<CodeEditorCompletionItem[]>
  onsignaturehelp?: (offset: number) => Promise<CodeEditorSignatureHelp | null>
  onViewReady?: (view: EditorView | null) => void
}
