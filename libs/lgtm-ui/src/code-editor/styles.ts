// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

export const codeEditorRootVariants = cva(
  [
    'helical-code-editor relative w-full min-h-0 flex-1 overflow-hidden bg-background text-foreground font-mono outline-none',
    '[&_.cm-editor]:h-full [&_.cm-editor]:outline-none',
    '[&_.cm-scroller]:h-full [&_.cm-scroller]:overflow-auto [&_.cm-scroller]:font-mono',
    '[&_.cm-gutters]:border-r [&_.cm-gutters]:border-border [&_.cm-gutters]:bg-muted/40 [&_.cm-gutters]:text-muted-foreground',
    '[&_.cm-tooltip]:max-h-64 [&_.cm-tooltip]:max-w-lg [&_.cm-tooltip]:overflow-y-auto [&_.cm-tooltip]:overscroll-contain [&_.cm-tooltip]:rounded-md [&_.cm-tooltip]:border [&_.cm-tooltip]:border-border [&_.cm-tooltip]:bg-popover [&_.cm-tooltip]:text-popover-foreground [&_.cm-tooltip]:shadow-md',
    '[&_.cm-tooltip-section]:border-0 [&_.cm-tooltip-section]:bg-transparent [&_.cm-tooltip-section:not(:first-child)]:border-t [&_.cm-tooltip-section:not(:first-child)]:border-border',
    '[&_.cm-tooltip-lint]:m-0 [&_.cm-tooltip-lint]:p-0',
    '[&_.cm-diagnostic]:px-3 [&_.cm-diagnostic]:py-2 [&_.cm-diagnostic]:font-sans [&_.cm-diagnostic]:text-xs [&_.cm-diagnostic]:leading-relaxed',
  ].join(' '),
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
  'helical-code-editor-hover flex flex-col gap-1.5 px-3 py-2 font-sans text-xs leading-relaxed text-popover-foreground',
)
export type CodeEditorHoverTooltipVariants = VariantProps<typeof codeEditorHoverTooltipVariants>

export const codeEditorMarkdownStyles = {
  heading: 'font-semibold text-foreground leading-snug',
  paragraph: 'm-0 text-popover-foreground leading-relaxed',
  codeBlock:
    'm-0 overflow-x-auto rounded border border-border/70 bg-muted/60 px-2.5 py-1.5 font-mono text-[11px] leading-relaxed text-foreground',
  inlineCode:
    'rounded border border-border/60 bg-muted/70 px-1 py-0.5 font-mono text-[11px] text-foreground',
  blockquote: 'm-0 border-l-2 border-border pl-2.5 italic text-muted-foreground',
  list: 'm-0 list-disc space-y-0.5 pl-4 text-popover-foreground',
  orderedList: 'm-0 list-decimal space-y-0.5 pl-4 text-popover-foreground',
  hr: 'my-1 border-0 border-t border-border',
  link: 'text-sky-600 underline underline-offset-2 hover:text-sky-700',
  strong: 'font-semibold text-foreground',
} as const
