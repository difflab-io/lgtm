<script lang="ts">
  import type { TreeViewProps } from './types'
  import {
    TreeView as ArkTreeView,
    createTreeCollection,
    useTreeView,
  } from '@ark-ui/svelte/tree-view'
  import { tick, untrack } from 'svelte'
  import { cn, getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
  import { useKeymap } from '@cloudvoyant/helical-svelte'
  import TreeNodeView from './TreeNode.svelte'
  import type { TreeNode } from './types'
  import { treeViewRootBase, treeViewListBase } from './styles'

  // Props ---------------------------------------------------------------------
  let {
    nodes,
    showHidden = $bindable(false),
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
    getLabel = (node) => node.label,
    class: className = '',
  }: TreeViewProps = $props()

  // State ---------------------------------------------------------------------
  let rootEl = $state<HTMLDivElement | null>(null)
  let renameErrors = $state<Record<string, string>>({})
  let attempts = $state<Record<string, string>>({})
  // Replace the cache so collections also update for branches created after mount.
  let loaded = $state.raw<Record<string, TreeNode[]>>({})
  let errors = $state<Record<string, string>>({})
  let pendingChildFocusParent = $state<string | null>(null)
  const id = $props.id()

  // Derived -------------------------------------------------------------------
  const collection = $derived(
    createTreeCollection<TreeNode>({
      rootNode: {
        value: '__root__',
        label,
        children: nodes.filter(isVisibleNode).map(hydrate),
      },
      nodeToValue: (node) => node.value,
      nodeToString: (node) => node.label,
      nodeToChildrenCount: (node) => (node.branch ? (node.children?.length ?? 1) : undefined),
    }),
  )
  const tree = useTreeView<TreeNode>(() => ({
    id,
    collection,
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
            if (pendingChildFocusParent === node.value) {
              pendingChildFocusParent = null
              const firstChild = children[0]?.value
              if (firstChild) {
                void tick().then(() => {
                  tree().focus(firstChild)
                })
              }
            }
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

  useKeymap(
    {
      Home: {
        key: 'Home',
        label: 'Focus First Item',
        hidden: true,
        action: ({ event }) => {
          if (!event || !resolveTreeItemTarget(event)) return false
          const visible = tree()
            .getVisibleNodes()
            .map((v) => v.node)
          const dest = visible[0]?.value
          if (!dest) return false
          tree().focus(dest)
          return true
        },
      },
      End: {
        key: 'End',
        label: 'Focus Last Item',
        hidden: true,
        action: ({ event }) => {
          if (!event || !resolveTreeItemTarget(event)) return false
          const visible = tree()
            .getVisibleNodes()
            .map((v) => v.node)
          const dest = visible[visible.length - 1]?.value
          if (!dest) return false
          tree().focus(dest)
          return true
        },
      },
      j: {
        key: 'j',
        label: 'Next Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          return item ? moveFocusDelta(item.nodeValue, 1) : false
        },
      },
      ArrowDown: {
        key: 'ArrowDown',
        label: 'Next Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          return item ? moveFocusDelta(item.nodeValue, 1) : false
        },
      },
      k: {
        key: 'k',
        label: 'Previous Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          return item ? moveFocusDelta(item.nodeValue, -1) : false
        },
      },
      ArrowUp: {
        key: 'ArrowUp',
        label: 'Previous Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          return item ? moveFocusDelta(item.nodeValue, -1) : false
        },
      },
      l: {
        key: 'l',
        label: 'Expand Folder or Open File',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item) return false
          if (!item.isBranch) {
            onselect?.(item.nodeValue)
            return true
          }
          return expandOrFocusChild(item.nodeValue, item.isBranch)
        },
      },
      ArrowRight: {
        key: 'ArrowRight',
        label: 'Expand Folder',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          return item ? expandOrFocusChild(item.nodeValue, item.isBranch) : false
        },
      },
      h: {
        key: 'h',
        label: 'Collapse Folder',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          return item ? collapseOrFocusParent(item.nodeValue, item.isBranch) : false
        },
      },
      H: {
        key: 'H',
        label: 'Toggle Hidden Items',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item) return false
          showHidden = !showHidden
          void tick().then(() => {
            const visible = tree()
              .getVisibleNodes()
              .map((v) => v.node)
            const next = visible.find((n) => n.value === item.nodeValue) ?? visible[0]
            if (next) tree().focus(next.value)
          })
          return true
        },
      },
      ArrowLeft: {
        key: 'ArrowLeft',
        label: 'Collapse Folder',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          return item ? collapseOrFocusParent(item.nodeValue, item.isBranch) : false
        },
      },
      Enter: {
        key: 'Enter',
        label: 'Open File or Toggle Folder',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item) return false
          if (item.isBranch) {
            if (tree().expandedValue.includes(item.nodeValue)) {
              tree().collapse([item.nodeValue])
            } else {
              tree().expand([item.nodeValue])
            }
            return true
          }
          onselect?.(item.nodeValue)
          return true
        },
      },
      r: {
        key: 'r',
        label: 'Rename Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady() && Boolean(onrename),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item || !onrename) return false
          tree().startRenaming(item.nodeValue)
          return true
        },
      },
      F2: {
        key: 'F2',
        label: 'Rename Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady() && Boolean(onrename),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item || !onrename) return false
          tree().startRenaming(item.nodeValue)
          return true
        },
      },
      d: {
        key: 'd',
        label: 'Delete Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady() && Boolean(onaction),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item || !onaction) return false
          onaction(item.nodeValue, 'delete')
          return true
        },
      },
      Delete: {
        key: 'Delete',
        label: 'Delete Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady() && Boolean(onaction),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item || !onaction) return false
          onaction(item.nodeValue, 'delete')
          return true
        },
      },
      a: {
        key: 'a',
        label: 'New File',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady() && Boolean(onaction),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item || !onaction) return false
          const parentFolder = item.isBranch
            ? item.nodeValue
            : item.nodeValue.includes('/')
              ? item.nodeValue.slice(0, item.nodeValue.lastIndexOf('/'))
              : ''
          onaction(parentFolder, 'new-file')
          return true
        },
      },
      A: {
        key: 'A',
        label: 'New Folder',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady() && Boolean(onaction),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item || !onaction) return false
          const parentFolder = item.isBranch
            ? item.nodeValue
            : item.nodeValue.includes('/')
              ? item.nodeValue.slice(0, item.nodeValue.lastIndexOf('/'))
              : ''
          onaction(parentFolder, 'new-folder')
          return true
        },
      },
    },
    {
      priority: 'pane',
      element: () => rootEl?.closest<HTMLElement>('[data-pane-id]') ?? rootEl,
      enabled: () => Boolean(rootEl),
    },
  )

  // Methods -------------------------------------------------------------------
  function resolveTreeItemTarget(
    event: KeyboardEvent,
  ): { nodeValue: string; isBranch: boolean } | null {
    const target = event.target
    const paneEl = rootEl?.closest<HTMLElement>('[data-pane-id]') ?? rootEl
    if (!(target instanceof HTMLElement) || !paneEl?.contains(target)) return null
    if (
      target.closest('[data-part="node-rename-input"]') ||
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target.isContentEditable
    ) {
      return null
    }
    const itemEl =
      target.closest<HTMLElement>('[data-which-key-target][data-value]') ??
      (target === paneEl || target.getAttribute('role') === 'tree'
        ? (rootEl?.querySelector<HTMLElement>(
            '[data-pane-focus-target][data-which-key-target][data-value]',
          ) ?? rootEl?.querySelector<HTMLElement>('[data-which-key-target][data-value]'))
        : null)
    if (!itemEl) return null
    const nodeValue = itemEl.dataset.value
    if (!nodeValue) return null
    return {
      nodeValue,
      isBranch: Boolean(collection.findNode(nodeValue)?.branch),
    }
  }

  function isNormalTreeReady(): boolean {
    if (!rootEl) return false
    const controller = getDocumentWhichKeyController(rootEl.ownerDocument)
    if (!controller) return true
    const state = controller.getState()
    return (
      state.mode === 'normal' &&
      state.activePath.length === 0 &&
      !state.explorerOpen &&
      state.countBuffer.length === 0
    )
  }

  function moveFocusDelta(nodeValue: string, delta: 1 | -1): boolean {
    const visible = tree()
      .getVisibleNodes()
      .map((v) => v.node)
    const idx = visible.findIndex((n) => n.value === nodeValue)
    if (idx === -1) return false
    const next = visible[Math.max(0, Math.min(visible.length - 1, idx + delta))]
    if (next) tree().focus(next.value)
    return true
  }

  function expandOrFocusChild(nodeValue: string, isBranch: boolean): boolean {
    if (!isBranch) return false
    if (!tree().expandedValue.includes(nodeValue)) {
      tree().expand([nodeValue])
    } else {
      const visible = tree()
        .getVisibleNodes()
        .map((v) => v.node)
      const idx = visible.findIndex((n) => n.value === nodeValue)
      const child = visible[idx + 1]
      if (child && child.value.startsWith(`${nodeValue}/`)) {
        pendingChildFocusParent = null
        tree().focus(child.value)
      } else if (!loaded[nodeValue]) {
        pendingChildFocusParent = nodeValue
      }
    }
    return true
  }

  function collapseOrFocusParent(nodeValue: string, isBranch: boolean): boolean {
    if (isBranch && tree().expandedValue.includes(nodeValue)) {
      tree().collapse([nodeValue])
    } else if (nodeValue.includes('/')) {
      const parent = nodeValue.slice(0, nodeValue.lastIndexOf('/'))
      tree().focus(parent)
    }
    return true
  }

  function clearRename(value: string) {
    delete renameErrors[value]
    delete attempts[value]
  }
  function isVisibleNode(node: TreeNode): boolean {
    return (
      showHidden ||
      !node.label.startsWith('.') ||
      node.value === pendingNode?.value ||
      node.value === renameValue
    )
  }
  function hydrate(node: TreeNode): TreeNode {
    const base = loaded[node.value] ?? node.children
    const children =
      pendingNode && parentValue === node.value ? [pendingNode, ...(base ?? [])] : base
    return {
      ...node,
      displayLabel: getLabel(node),
      label: attempts[node.value] ?? node.label,
      children: children?.filter(isVisibleNode).map(hydrate),
    }
  }
</script>

<div bind:this={rootEl} class={cn(treeViewRootBase, className)}>
  <ArkTreeView.RootProvider value={tree} class={treeViewRootBase} lazyMount unmountOnExit>
    <ArkTreeView.Tree aria-label={label} class={treeViewListBase}>
      {#each collection.rootNode.children ?? [] as node, index (node.value)}
        <TreeNodeView
          {node}
          indexPath={[index]}
          {errors}
          {renameErrors}
          {icon}
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
</div>
