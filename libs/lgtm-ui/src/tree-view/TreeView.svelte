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
    version,
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
    selectableBranches = false,
    expandedValue,
    onExpandedChange,
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
    ...(expandedValue !== undefined ? { expandedValue } : {}),
    expandOnClick: !selectableBranches,
    selectionMode: 'single',
    canRename: (node) => Boolean(onrename) && node.renamable !== false,
    onExpandedChange: ({ expandedValue: next }) => {
      onExpandedChange?.(next)
    },

    onRenameComplete: ({ value, label }) => {
      if (!onrename) return
      void (async () => {
        try {
          await onrename(value, label)
          clearRename(value)
          getDocumentWhichKeyController(rootEl?.ownerDocument)?.setMode('normal')
        } catch (error) {
          renameErrors[value] = error instanceof Error ? error.message : String(error)
          attempts[value] = label
          await tick()
          getDocumentWhichKeyController(rootEl?.ownerDocument)?.setMode('insert')
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
      if (node && (selectableBranches || !node.branch)) onselect?.(node.value)
    },
  }))
  // Lifecycle -----------------------------------------------------------------
  let lastVersion: number | undefined
  $effect(() => {
    if (version !== undefined && version !== lastVersion) {
      lastVersion = version
      loaded = {}
      errors = {}
    }
  })
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
    const dirs = expandedValue
    const loader = loadChildren
    const _v = version
    if (!dirs?.length || !loader) return
    const currentLoaded = untrack(() => loaded)
    const missing = [...dirs]
      .filter(
        (dir) => !(dir in currentLoaded) && !untrack(() => collection.findNode(dir)?.children),
      )
      .sort((a, b) => a.split('/').length - b.split('/').length)
    if (missing.length === 0) return
    void (async () => {
      const results = await Promise.all(
        missing.map(async (dir) => {
          try {
            const node = untrack(() => collection.findNode(dir)) ?? {
              value: dir,
              label: dir.split('/').at(-1) ?? dir,
              branch: true,
            }
            const children = await loader(node)
            return { dir, children, error: null as string | null }
          } catch (error) {
            return {
              dir,
              children: null,
              error: error instanceof Error ? error.message : String(error),
            }
          }
        }),
      )
      const nextLoaded: Record<string, TreeNode[]> = {}
      let hasLoaded = false
      for (const item of results) {
        if (item.children) {
          nextLoaded[item.dir] = item.children
          hasLoaded = true
        } else if (item.error) {
          errors[item.dir] = item.error
        }
      }
      if (hasLoaded) {
        loaded = { ...loaded, ...nextLoaded }
      }
    })()
  })
  $effect(() => {
    const value = renameValue
    const parent = parentValue
    if (value)
      void tick().then(() => {
        if (parent) tree().expand([parent])
        getDocumentWhichKeyController(rootEl?.ownerDocument)?.setMode('insert')
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
          const visible = getVisibleNodeValues()
          const dest = visible[0]
          if (!dest) return false
          focusNodeValue(dest)
          return true
        },
      },
      End: {
        key: 'End',
        label: 'Focus Last Item',
        hidden: true,
        action: ({ event }) => {
          if (!event || !resolveTreeItemTarget(event)) return false
          const visible = getVisibleNodeValues()
          const dest = visible[visible.length - 1]
          if (!dest) return false
          focusNodeValue(dest)
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
        label: 'Open Item or Toggle Folder',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady(),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item) return false
          if (item.isBranch && !selectableBranches) {
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
          const node = collection.findNode(item.nodeValue)
          if (!node || node.renamable === false) return false
          getDocumentWhichKeyController(rootEl?.ownerDocument)?.setMode('insert')
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
          const node = collection.findNode(item.nodeValue)
          if (!node || node.renamable === false) return false
          getDocumentWhichKeyController(rootEl?.ownerDocument)?.setMode('insert')
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
        label: 'New Item',
        mode: 'normal',
        hidden: true,
        when: () => isNormalTreeReady() && Boolean(onaction),
        action: ({ event }) => {
          const item = event ? resolveTreeItemTarget(event) : null
          if (!item || !onaction) return false
          const parentFolder =
            selectableBranches || item.isBranch
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
          const parentFolder =
            selectableBranches || item.isBranch
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
  function getVisibleNodeValues(): string[] {
    if (selectableBranches && rootEl) {
      return Array.from(rootEl.querySelectorAll<HTMLElement>('[data-tree-node-value]'))
        .filter((el) => !el.closest('[hidden]'))
        .map((el) => el.dataset.treeNodeValue!)
        .filter(Boolean)
    }
    return tree()
      .getVisibleNodes()
      .map((v) => v.node.value)
  }

  function focusNodeValue(nodeValue: string) {
    tree().focus(nodeValue)
    if (selectableBranches && rootEl) {
      const el = rootEl.querySelector<HTMLElement>(
        `[data-tree-node-value=${JSON.stringify(nodeValue)}]`,
      )
      el?.focus()
    }
  }

  function resolveTreeItemTarget(
    event: KeyboardEvent,
  ): { nodeValue: string; isBranch: boolean } | null {
    const target = event.target
    const paneEl = rootEl?.closest<HTMLElement>('[data-pane-id]') ?? rootEl
    if (!(target instanceof HTMLElement) || !paneEl?.contains(target)) return null
    if (
      target.closest('[data-part="node-rename-input"]') ||
      target.closest('[role="dialog"]') ||
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target.isContentEditable
    ) {
      return null
    }
    const itemEl =
      target.closest<HTMLElement>('[data-tree-node-value]') ??
      (target === paneEl || target.getAttribute('role') === 'tree'
        ? (rootEl?.querySelector<HTMLElement>('[data-pane-focus-target][data-tree-node-value]') ??
          rootEl?.querySelector<HTMLElement>('[data-tree-node-value]'))
        : null)
    if (!itemEl) return null
    const nodeValue = itemEl.dataset.treeNodeValue
    if (!nodeValue) return null
    return {
      nodeValue,
      isBranch: itemEl.dataset.treeNodeBranch === 'true',
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
    const visible = getVisibleNodeValues()
    const idx = visible.indexOf(nodeValue)
    if (idx === -1) return false
    const next = visible[Math.max(0, Math.min(visible.length - 1, idx + delta))]
    if (next) focusNodeValue(next)
    return true
  }

  function expandOrFocusChild(nodeValue: string, isBranch: boolean): boolean {
    if (!isBranch) return false
    if (!tree().expandedValue.includes(nodeValue)) {
      tree().expand([nodeValue])
    } else {
      const visible = getVisibleNodeValues()
      const idx = visible.indexOf(nodeValue)
      const child = visible[idx + 1]
      if (child) {
        focusNodeValue(child)
      }
    }
    return true
  }

  function collapseOrFocusParent(nodeValue: string, isBranch: boolean): boolean {
    if (isBranch && tree().expandedValue.includes(nodeValue)) {
      tree().collapse([nodeValue])
      return true
    }
    const parentBranch = nodes.find((n) => n.children?.some((c) => c.value === nodeValue))
    if (parentBranch) {
      focusNodeValue(parentBranch.value)
      return true
    }
    if (nodeValue.includes('/')) {
      const parent = nodeValue.slice(0, nodeValue.lastIndexOf('/'))
      focusNodeValue(parent)
      return true
    }
    return false
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
  <ArkTreeView.RootProvider value={tree} class={treeViewRootBase}>
    <ArkTreeView.Tree aria-label={label} class={treeViewListBase}>
      {#each collection.rootNode.children ?? [] as node, index (node.value)}
        <TreeNodeView
          {node}
          indexPath={[index]}
          {errors}
          {renameErrors}
          {icon}
          {selectableBranches}
          {onselect}
          onedit={(value) => {
            delete renameErrors[value]
          }}
          oncancel={(value) => {
            clearRename(value)
            getDocumentWhichKeyController(rootEl?.ownerDocument)?.setMode('normal')
            oncancel?.(value)
          }}
          {contextItems}
          {onaction}
        />
      {/each}
    </ArkTreeView.Tree>
  </ArkTreeView.RootProvider>
</div>
