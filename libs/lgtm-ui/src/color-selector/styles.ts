// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

export const colorSelectorRootBase = 'inline-flex flex-wrap items-center gap-2'

export const colorSelectorItemVariants = cva(
  'helical-color-swatch relative inline-flex shrink-0 cursor-pointer items-center justify-center rounded-full border border-border/60 transition-transform outline-none hover:scale-110 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background data-[state=on]:ring-2 data-[state=on]:ring-foreground/75 data-[state=on]:ring-offset-1 data-[state=on]:ring-offset-background',
  {
    variants: {
      size: {
        sm: 'size-3.5',
        md: 'size-4',
      },
    },
    defaultVariants: { size: 'md' },
  },
)
export type ColorSelectorItemVariants = VariantProps<typeof colorSelectorItemVariants>

export const colorSelectorIndicatorBase = 'pointer-events-none size-2.5 text-white drop-shadow-xs'
