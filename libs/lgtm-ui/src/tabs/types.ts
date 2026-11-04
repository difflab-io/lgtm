import type { TabsListVariants, TabsTriggerVariants } from './styles'

export interface TabItem {
  value: string
  label: string
  dirty?: boolean
  closable?: boolean
}

export interface TabsProps {
  items: TabItem[]
  value: string | null
  ariaLabel?: string
  size?: TabsListVariants['size'] & TabsTriggerVariants['size']
  class?: string
  onchange: (value: string) => void
  onclose?: (value: string) => void
}
