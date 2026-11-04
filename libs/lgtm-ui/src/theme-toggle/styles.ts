// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

export const themeToggleVariants = cva(
  'helical-theme-toggle inline-flex shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring/60',
  {
    variants: {
      size: {
        sm: 'size-6.5 p-1',
        md: 'size-8 p-1.5',
      },
      variant: {
        ghost: 'border-0 bg-transparent',
        outline: 'border border-border bg-background',
      },
    },
    defaultVariants: {
      size: 'sm',
      variant: 'ghost',
    },
  },
)
export type ThemeToggleVariants = VariantProps<typeof themeToggleVariants>
