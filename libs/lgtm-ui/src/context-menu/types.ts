import type { Snippet } from 'svelte'
import type { ContextMenuItemVariants } from './styles'

export interface ContextMenuItem {
  value: string
  label: string
  shortcut?: string
  destructive?: boolean
}

export interface ContextMenuProps {
  children: Snippet<[Record<string, unknown>]>
  items: ContextMenuItem[]
  size?: ContextMenuItemVariants['size']
  class?: string
  onaction: (value: string) => void
}

export interface ContextMenuPositionerProps {
  children?: Snippet
}
