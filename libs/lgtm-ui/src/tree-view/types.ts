import type { Snippet } from 'svelte'
import type { ContextMenuItem } from '../context-menu/types'

export interface TreeNode {
  value: string
  displayLabel?: string
  label: string
  description?: string
  branch?: boolean
  children?: TreeNode[]
}

export interface TreeViewProps {
  refresh?: { path: string; version: number }
  pendingNode?: TreeNode
  parentValue?: string
  contextItems?: (node: TreeNode) => ContextMenuItem[]
  onaction?: (value: string, action: string) => void
  renameValue?: string
  onrename?: (value: string, name: string) => Promise<void>
  oncancel?: (value: string) => void
  icon?: Snippet<[TreeNode, boolean]>
  getLabel?: (node: TreeNode) => string
  nodes: TreeNode[]
  showHidden?: boolean
  selected?: string | null
  onselect?: (value: string) => void
  loadChildren?: (node: TreeNode) => Promise<TreeNode[]>
  label?: string
  class?: string
}

export interface TreeNodeViewProps {
  node: TreeNode
  indexPath: number[]
  errors: Record<string, string>
  renameErrors: Record<string, string>
  icon?: Snippet<[TreeNode, boolean]>
  onedit?: (value: string) => void
  contextItems?: (node: TreeNode) => ContextMenuItem[]
  onaction?: (value: string, action: string) => void
  oncancel?: (value: string) => void
}
