// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

/**
 * Root CVA variants for the `DiffEditor` component.
 */
export const diffEditorRootVariants = cva(
  [
    'helical-diff-editor relative w-full flex-1 overflow-auto bg-background text-foreground font-mono outline-none',
    '[&_.cm-editor]:h-full [&_.cm-editor]:outline-none',
    '[&_.cm-mergeView]:h-full [&_.cm-mergeView]:w-full [&_.cm-mergeView]:overflow-auto',
    '[&_.cm-mergeViewEditors]:flex [&_.cm-mergeViewEditors]:min-h-full [&_.cm-mergeViewEditors]:w-full [&_.cm-mergeViewEditors]:items-stretch',
    '[&_.cm-mergeViewEditor]:min-w-0 [&_.cm-mergeViewEditor]:flex-1 [&_.cm-mergeViewEditor:first-child]:border-r [&_.cm-mergeViewEditor:first-child]:border-border',
    '[&_.cm-scroller]:font-mono',
    '[&_.cm-gutters]:border-r [&_.cm-gutters]:border-border [&_.cm-gutters]:bg-muted/40 [&_.cm-gutters]:text-muted-foreground',
    '[&_.cm-collapsedLines]:border-y [&_.cm-collapsedLines]:border-border [&_.cm-collapsedLines]:bg-muted/50 [&_.cm-collapsedLines]:px-3 [&_.cm-collapsedLines]:py-1 [&_.cm-collapsedLines]:font-sans [&_.cm-collapsedLines]:text-[11px] [&_.cm-collapsedLines]:text-muted-foreground',
  ].join(' '),
  {
    variants: {
      size: {
        sm: 'text-xs [&_.cm-scroller]:text-xs [&_.cm-scroller]:leading-relaxed',
        md: 'text-sm [&_.cm-scroller]:text-sm [&_.cm-scroller]:leading-relaxed',
      },
      mode: {
        unified: 'helical-diff-editor-unified',
        split: 'helical-diff-editor-split',
      },
    },
    defaultVariants: {
      size: 'sm',
      mode: 'split',
    },
  },
)

/** Variant props for `diffEditorRootVariants`. */
export type DiffEditorRootVariants = VariantProps<typeof diffEditorRootVariants>

/** Alias for `diffEditorRootVariants`. */
export const diffEditorStyles = diffEditorRootVariants
export type DiffEditorVariantProps = DiffEditorRootVariants
