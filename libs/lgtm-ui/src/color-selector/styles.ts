// Helical convention: framework-neutral classes and CVA variants, consumed with cn.
import { cva, type VariantProps } from 'class-variance-authority'

export const colorSelectorRootBase = 'inline-flex flex-wrap items-center gap-2 py-0.5'

export const colorSelectorItemVariants = cva(
  'helical-color-swatch relative inline-flex shrink-0 cursor-pointer items-center justify-center rounded-full border border-black/15 shadow-2xs transition-all duration-150 outline-none dark:border-white/20 hover:scale-110 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background data-[state=on]:scale-105 data-[state=on]:ring-2 data-[state=on]:ring-foreground data-[state=on]:ring-offset-2 data-[state=on]:ring-offset-background',
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
