// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

export const menuPositionerBase = 'z-[100] [--z-index:100]!'

export const menuTriggerBase =
  'inline-flex cursor-pointer items-center justify-center gap-1.5 rounded border border-border bg-background px-1.5 py-0.5 text-[10px] text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-slate-500'

export const menuContentBase =
  'z-50 min-w-52 max-w-80 rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md outline-none'

export const menuItemVariants = cva(
  'flex w-full cursor-default select-none items-center justify-between gap-3 rounded-sm outline-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
  {
    variants: {
      size: {
        sm: 'min-h-6.5 px-2 py-1 text-xs',
        md: 'min-h-8 px-2.5 py-1.5 text-sm',
      },
      destructive: {
        true: 'text-destructive data-[highlighted]:text-destructive',
      },
    },
    defaultVariants: { size: 'sm', destructive: false },
  },
)
export type MenuItemVariants = VariantProps<typeof menuItemVariants>

export const menuIndicatorVariants = cva('inline-block size-2 shrink-0 rounded-full', {
  variants: {
    tone: {
      'active-page': 'bg-emerald-500',
      'active-project': 'bg-sky-500',
      warning: 'bg-amber-500',
      error: 'bg-rose-500',
      inactive: 'bg-slate-300',
    },
  },
  defaultVariants: { tone: 'inactive' },
})
export type MenuIndicatorVariants = VariantProps<typeof menuIndicatorVariants>

export const menuItemMainBase = 'pointer-events-none flex min-w-0 flex-1 items-center gap-2'
export const menuItemLabelBase = 'truncate font-medium'
export const menuItemDetailBase =
  'pointer-events-none ml-auto shrink-0 text-[10px] text-muted-foreground'
export const menuSeparatorBase = 'my-1 h-px bg-border'
