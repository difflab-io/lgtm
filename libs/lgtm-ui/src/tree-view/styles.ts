import { cn, inputVariants, sidebarMenuButtonVariants } from '@cloudvoyant/helical-ui'
import { cva, type VariantProps } from 'class-variance-authority'

export const treeViewRootBase = 'helical-tree-view'
export const treeViewListBase = 'm-0 list-none p-0'
export const treeViewBranchContentBase = cn(treeViewListBase, 'relative')
export const treeViewLabelBase = 'tree-label truncate'
export const treeViewIconBase =
  'tree-icon inline-flex size-3.5 shrink-0 items-center justify-center'
export const treeViewIndentGuideBase =
  'tree-indent-guide pointer-events-none absolute inset-y-0 left-[calc(17px_+_(var(--depth)_-_1)_*_16px)] border-l border-sidebar-border'

export const treeViewRowVariants = cva(
  cn(
    sidebarMenuButtonVariants({ size: 'sm' }),
    'helical-tree-row h-6 cursor-pointer py-0 ps-[calc(10px_+_(var(--depth)_-_1)_*_16px)] text-xs data-[selected]:bg-sidebar-accent data-[selected]:text-sidebar-accent-foreground',
  ),
)

export const treeViewRenameInputBase = cn(
  inputVariants({ size: 'sm' }),
  'h-5.5 rounded-xs border-sidebar-ring bg-background px-0.75 text-inherit shadow-none focus-visible:ring-1 aria-invalid:ring-1 aria-invalid:ring-destructive/20 data-invalid:ring-1 data-invalid:ring-destructive/20',
)

export const treeViewMessageVariants = cva('tree-message mx-3 my-1 text-[11px]', {
  variants: { variant: { default: 'text-muted-foreground', error: 'text-destructive' } },
  defaultVariants: { variant: 'default' },
})
export type TreeViewMessageVariants = VariantProps<typeof treeViewMessageVariants>
