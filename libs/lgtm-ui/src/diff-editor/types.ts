import type {
  CodeEditorDiagnostic,
  CodeEditorHoverResult,
  CodeEditorLineWrapping,
} from '../code-editor/types'
import type { DiffEditorRootVariants } from './styles'

/**
 * Column layout mode for `DiffEditor`.
 * - `'unified'`: Single-column inline diff (`unifiedMergeView`).
 * - `'split'`: Synchronized side-by-side `a` (base) and `b` (head) columns (`MergeView`).
 */
export type DiffEditorMode = 'unified' | 'split'

/**
 * Props accepted by the `DiffEditor` component.
 */
export interface DiffEditorProps {
  /** Base (`a`) document text. */
  baseValue: string
  /** Head (`b`) document text. */
  headValue?: string
  /** Workspace-relative file path or filename used for syntax detection. */
  filename?: string
  /** Active diff layout (`'unified'` or `'split'`). */
  mode?: DiffEditorMode
  /** When `true`, renders the full uncollapsed file. When `false`, collapses unchanged regions around diff hunks. */
  showFullFile?: boolean
  /** When `true`, locks both base and head editors against modification. When `false`, allows live editing in the head (`b`) editor. */
  readonly?: boolean
  /** Line wrapping mode applied to the diff editor panes. */
  lineWrapping?: CodeEditorLineWrapping
  /** Number of unchanged context lines visible around each diff hunk when `showFullFile` is `false`. */
  collapseUnchangedMargin?: number
  /** Minimum number of unchanged lines required before collapsing a block when `showFullFile` is `false`. */
  collapseUnchangedMinSize?: number
  /** Font size variant (`'sm'` or `'md'`). */
  size?: DiffEditorRootVariants['size']
  /** Accessible label for the diff editor region. */
  label?: string
  /** Optional class names merged onto the root container. */
  class?: string
  /** Optional LSP diagnostics rendered on the head (`b`) document. */
  diagnostics?: CodeEditorDiagnostic[]
  /** Callback fired when the editable head (`b`) document changes. */
  onchange?: (value: string) => void
  /** Callback fired when the user triggers save (`Mod-s`) in the head (`b`) document. */
  onsave?: (value: string) => void
  /** Optional hover provider for the head (`b`) document. */
  onhover?: (offset: number) => Promise<CodeEditorHoverResult | null>
  /** Optional go-to-definition handler for the head (`b`) document. */
  ongotodefinition?: (offset: number) => void | Promise<void>
  /** Callback fired when the layout mode (`'unified'` or `'split'`) changes. */
  onmodetoggle?: (mode: DiffEditorMode) => void
  /** Callback fired when the full-file versus hunk-only toggle (`showFullFile`) changes. */
  onfullfiletoggle?: (showFullFile: boolean) => void
}
