// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

export const tabsRootBase = 'w-full shrink-0'

export const tabsListVariants = cva(
  'flex w-full items-stretch overflow-x-auto border-b border-border bg-slate-50 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden',
  {
    variants: {
      size: {
        sm: 'h-8.5',
        md: 'h-10',
      },
    },
    defaultVariants: { size: 'sm' },
  },
)
export type TabsListVariants = VariantProps<typeof tabsListVariants>

export const tabsItemContainerVariants = cva(
  'inline-flex h-full shrink-0 items-stretch border-r border-border text-muted-foreground transition-colors data-[selected]:bg-background data-[selected]:font-medium data-[selected]:text-foreground',
  {
    variants: {
      size: {
        sm: 'text-[11px]',
        md: 'text-xs',
      },
    },
    defaultVariants: { size: 'sm' },
  },
)

export const tabsTriggerVariants = cva(
  'inline-flex h-full cursor-pointer items-center gap-1.5 border-0 bg-transparent text-inherit whitespace-nowrap outline-none',
  {
    variants: {
      size: {
        sm: 'pl-3 pr-1.5 text-[11px]',
        md: 'pl-3.5 pr-2 text-xs',
      },
    },
    defaultVariants: { size: 'sm' },
  },
)
export type TabsTriggerVariants = VariantProps<typeof tabsTriggerVariants>

export const tabsDirtyIndicatorBase = 'inline-block size-1.5 shrink-0 rounded-full bg-amber-500'

export const tabsCloseButtonBase =
  'mr-1.5 inline-flex cursor-pointer items-center justify-center self-center rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground'
