// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

export const codeEditorRootVariants = cva(
  'helical-code-editor relative w-full flex-1 overflow-auto bg-background text-foreground font-mono outline-none [&_.cm-editor]:h-full [&_.cm-editor]:outline-none [&_.cm-scroller]:font-mono [&_.cm-gutters]:border-r [&_.cm-gutters]:border-border [&_.cm-gutters]:bg-muted/40 [&_.cm-gutters]:text-muted-foreground',
  {
    variants: {
      size: {
        sm: 'text-xs [&_.cm-scroller]:text-xs [&_.cm-scroller]:leading-relaxed',
        md: 'text-sm [&_.cm-scroller]:text-sm [&_.cm-scroller]:leading-relaxed',
      },
    },
    defaultVariants: { size: 'sm' },
  },
)
export type CodeEditorRootVariants = VariantProps<typeof codeEditorRootVariants>

export const codeEditorHoverTooltipVariants = cva(
  'helical-code-editor-hover max-w-md rounded-md border border-border bg-popover px-3 py-2 font-mono text-xs text-popover-foreground shadow-md whitespace-pre-wrap',
)
export type CodeEditorHoverTooltipVariants = VariantProps<typeof codeEditorHoverTooltipVariants>
