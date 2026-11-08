<script lang="ts">
  import type { TreeViewProps } from './types'
  import {
    TreeView as ArkTreeView,
    createTreeCollection,
    useTreeView,
  } from '@ark-ui/svelte/tree-view'
  import { tick, untrack } from 'svelte'
  import { cn } from '@cloudvoyant/helical-ui'
  import TreeNodeView from './TreeNode.svelte'
  import type { TreeNode } from './types'
  import { treeViewRootBase, treeViewListBase } from './styles'

  // Props ---------------------------------------------------------------------
  let {
    nodes,
    renameValue,
    onrename,
    oncancel,
    onaction,
    contextItems,
    pendingNode,
    refresh,
    parentValue,
    selected = null,
    onselect,
    loadChildren,
    label = 'Files',
    icon,
    trailing,
    defaultExpandedValue,
    getLabel = (node) => node.label,
    class: className = '',
  }: TreeViewProps = $props()

  // State ---------------------------------------------------------------------
  let renameErrors = $state<Record<string, string>>({})
  let attempts = $state<Record<string, string>>({})
  // Replace the cache so collections also update for branches created after mount.
  let loaded = $state.raw<Record<string, TreeNode[]>>({})
  let errors = $state<Record<string, string>>({})
  const id = $props.id()

  // Derived -------------------------------------------------------------------
  const rootClasses = $derived(cn(treeViewRootBase, className))
  const collection = $derived(
    createTreeCollection<TreeNode>({
      rootNode: { value: '__root__', label, children: nodes.map(hydrate) },
      nodeToValue: (node) => node.value,
      nodeToString: (node) => node.label,
      nodeToChildrenCount: (node) => (node.branch ? (node.children?.length ?? 1) : undefined),
    }),
  )
  const tree = useTreeView<TreeNode>(() => ({
    id,
    collection,
    defaultExpandedValue,
    selectedValue: selected ? [selected] : [],
    selectionMode: 'single',
    canRename: () => Boolean(onrename),

    onRenameComplete: ({ value, label }) => {
      if (!onrename) return
      void (async () => {
        try {
          await onrename(value, label)
          clearRename(value)
        } catch (error) {
          renameErrors[value] = error instanceof Error ? error.message : String(error)
          attempts[value] = label
          await tick()
          tree().startRenaming(value)
        }
      })()
    },
    loadChildren: loadChildren
      ? async ({ node }) => {
          delete errors[node.value]
          try {
            const children = await loadChildren(node)
            loaded = { ...loaded, [node.value]: children }
            return children
          } catch (error) {
            errors[node.value] = error instanceof Error ? error.message : String(error)
            throw error
          }
        }
      : undefined,
    onSelectionChange: ({ selectedValue }) => {
      const value = selectedValue[0]
      const node = value ? collection.findNode(value) : undefined
      if (node && !node.branch) onselect?.(node.value)
    },
  }))
  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    const request = refresh
    if (request && loadChildren) {
      const node = untrack(() => collection.findNode(request.path))
      if (node)
        void loadChildren(node)
          .then(async (children) => {
            loaded = { ...loaded, [request.path]: children }
            await tick()
            tree().expand([request.path])
          })
          .catch((error) => {
            errors[request.path] = String(error)
          })
    }
  })
  $effect(() => {
    const value = renameValue
    const parent = parentValue
    if (value)
      void tick().then(() => {
        if (parent) tree().expand([parent])
        tree().startRenaming(value)
      })
  })

  // Methods -------------------------------------------------------------------
  function clearRename(value: string) {
    delete renameErrors[value]
    delete attempts[value]
  }
  function hydrate(node: TreeNode): TreeNode {
    const base = loaded[node.value] ?? node.children
    const children =
      pendingNode && parentValue === node.value ? [pendingNode, ...(base ?? [])] : base
    return {
      ...node,
      displayLabel: getLabel(node),
      label: attempts[node.value] ?? node.label,
      children: children?.map(hydrate),
    }
  }
</script>

<ArkTreeView.RootProvider value={tree} class={rootClasses} lazyMount unmountOnExit>
  <ArkTreeView.Tree aria-label={label} class={treeViewListBase}>
    {#each collection.rootNode.children ?? [] as node, index (node.value)}
      <TreeNodeView
        {node}
        indexPath={[index]}
        {errors}
        {renameErrors}
        {icon}
        {trailing}
        onedit={(value) => {
          delete renameErrors[value]
        }}
        oncancel={(value) => {
          clearRename(value)
          oncancel?.(value)
        }}
        {contextItems}
        {onaction}
      />
    {/each}
  </ArkTreeView.Tree>
</ArkTreeView.RootProvider>
