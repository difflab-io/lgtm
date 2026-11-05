// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

export const contextMenuPositionerBase = 'z-[100] [--z-index:100]!'
export const contextMenuContentBase =
  'z-50 min-w-40 rounded-sm border border-border bg-popover px-0 py-0.5 text-popover-foreground shadow-md outline-none'

export const contextMenuItemVariants = cva(
  'flex cursor-default select-none items-center justify-between gap-4 rounded-none outline-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
  {
    variants: {
      size: { sm: 'min-h-5.5 px-2 py-0.5 text-[11px]', md: 'min-h-6.5 px-2.5 py-1 text-xs' },
      destructive: { true: 'text-destructive data-[highlighted]:text-destructive' },
    },
    defaultVariants: { size: 'sm', destructive: false },
  },
)
export type ContextMenuItemVariants = VariantProps<typeof contextMenuItemVariants>
export const contextMenuShortcutBase = 'text-[10px] text-muted-foreground'
