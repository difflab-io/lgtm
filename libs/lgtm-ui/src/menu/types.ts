import type { Snippet } from 'svelte'
import type { MenuIndicatorVariants, MenuItemVariants } from './styles'

export type MenuItemIndicatorTone = NonNullable<MenuIndicatorVariants['tone']>

export interface MenuItem {
  value: string
  label: string
  detail?: string
  shortcut?: string
  indicator?: MenuItemIndicatorTone
  indicatorLabel?: string
  disabled?: boolean
  destructive?: boolean
  separatorBefore?: boolean
  render?: Snippet<[MenuItem]>
  items?: MenuItem[]
}

export interface MenuProps {
  trigger: Snippet<[Record<string, unknown>]>
  items: MenuItem[]
  ariaLabel?: string
  triggerAriaLabel?: string
  triggerTitle?: string
  triggerClass?: string
  placement?: 'top-end' | 'top-start' | 'bottom-end' | 'bottom-start'
  size?: MenuItemVariants['size']
  class?: string
  onselect: (value: string) => void
  onopenchange?: (open: boolean) => void
}

export interface MenuPositionerProps {
  children?: Snippet
}
