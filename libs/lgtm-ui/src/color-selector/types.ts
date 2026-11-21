import type { ColorSelectorItemVariants } from './styles'

export interface ColorOption {
  value: string
  label: string
  color: string | null
}

export interface ColorSelectorProps {
  items: ColorOption[]
  value: string
  ariaLabel?: string
  size?: ColorSelectorItemVariants['size']
  class?: string
  onchange?: (value: string) => void
}
