<script lang="ts">
  import type { TreeNodeViewProps } from './types'
  import { ChevronDown, ChevronRight, Folder, FolderOpen, FileText } from 'lucide-svelte'
  import ContextMenu from '../context-menu/ContextMenu.svelte'
  import { TreeView as ArkTreeView, useTreeViewContext } from '@ark-ui/svelte/tree-view'
  import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
  import {
    treeViewListBase,
    treeViewBranchContentBase,
    treeViewRowVariants,
    treeViewLabelBase,
    treeViewIconBase,
    treeViewIndentGuideBase,
    treeViewRenameInputBase,
    treeViewMessageVariants,
  } from './styles'
  import TreeNodeView from './TreeNode.svelte'
  // Props ---------------------------------------------------------------------
  let {
    node,
    indexPath,
    errors,
    renameErrors,
    icon,
    selectableBranches = false,
    onselect,
    onedit,
    oncancel,
    contextItems,
    onaction,
  }: TreeNodeViewProps = $props()
  // State ---------------------------------------------------------------------
  const tree = useTreeViewContext()
  const id = $props.id()
  const renameErrorId = `${id}-rename-error`
  const rowClass = treeViewRowVariants()
  const isDefaultFocusTarget = $derived(
    indexPath.length === 1 && indexPath[0] === 0 && (tree().selectedValue?.length ?? 0) === 0,
  )

  // Methods -------------------------------------------------------------------
  function action(value: string) {
    // Let Ark close the menu and restore focus before starting another interaction.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (value === 'rename') {
          getDocumentWhichKeyController()?.setMode('insert')
          tree().startRenaming(node.value)
        } else onaction?.(node.value, value)
      }),
    )
  }
</script>

{#snippet nodeIcon(folder: boolean, expanded = false)}
  <span class={treeViewIconBase} aria-hidden="true">
    {#if icon}{@render icon(node, expanded)}{:else if folder}{#if expanded}<FolderOpen
          size={13}
          strokeWidth={1.5}
        />{:else}<Folder size={13} strokeWidth={1.5} />{/if}
    {:else}<FileText size={13} strokeWidth={1.5} />{/if}
  </span>
{/snippet}

<ArkTreeView.NodeProvider {node} {indexPath}>
  <ArkTreeView.NodeContext>
    {#snippet render(state)}
      {#if node.branch}
        <ArkTreeView.Branch aria-label={node.displayLabel ?? node.label} class={treeViewListBase}>
          <ContextMenu items={contextItems?.(node) ?? []} onaction={action}>
            {#snippet children(triggerProps)}
              <ArkTreeView.BranchControl
                {...triggerProps}
                class={rowClass}
                title={node.description ?? node.value}
                data-which-key-target=""
                data-pane-focus-target={state().selected || isDefaultFocusTarget ? '' : undefined}
                data-tree-node-value={node.value}
                data-tree-node-branch="true"
                onclickcapture={(event) => {
                  if (!selectableBranches) return
                  if (
                    event.target instanceof Element &&
                    (event.target.closest('[data-branch-toggle]') ||
                      event.target.closest('[data-part="node-rename-input"]'))
                  ) {
                    return
                  }
                  event.preventDefault()
                  event.stopPropagation()
                  onselect?.(node.value)
                }}
              >
                <div class="flex min-w-0 flex-1 items-center gap-1.5">
                  {@render nodeIcon(true, state().expanded)}
                  <ArkTreeView.BranchText class={treeViewLabelBase} hidden={state().renaming}
                    >{node.displayLabel ?? node.label}</ArkTreeView.BranchText
                  >
                </div>
                {#if selectableBranches}
                  <button
                    type="button"
                    data-branch-toggle
                    class="ms-auto inline-flex size-4 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
                    aria-label={state().expanded
                      ? `Collapse ${node.label}`
                      : `Expand ${node.label}`}
                    onclick={(event) => {
                      event.preventDefault()
                      event.stopPropagation()
                      if (state().expanded) tree().collapse([node.value])
                      else tree().expand([node.value])
                    }}
                  >
                    {#if state().expanded}
                      <ChevronDown size={12} strokeWidth={1.5} />
                    {:else}
                      <ChevronRight size={12} strokeWidth={1.5} />
                    {/if}
                  </button>
                {/if}
                <ArkTreeView.NodeRenameInput
                  aria-label="Name"
                  data-colon-command=""
                  class={treeViewRenameInputBase}
                  aria-invalid={Boolean(renameErrors[node.value])}
                  data-invalid={renameErrors[node.value] ? '' : undefined}
                  aria-describedby={renameErrors[node.value] ? renameErrorId : undefined}
                  oninput={() => onedit?.(node.value)}
                  onkeydown={(event) => {
                    if (event.key === 'Escape') oncancel?.(node.value)
                  }}
                />
              </ArkTreeView.BranchControl>
            {/snippet}
          </ContextMenu>
          {#if errors[node.value]}<p class={treeViewMessageVariants()} role="alert">
              {errors[node.value]} Expand to retry.
            </p>{/if}
          <ArkTreeView.BranchContent
            class={treeViewBranchContentBase}
            hidden={selectableBranches ? (node.children?.length ?? 0) === 0 : undefined}
          >
            <ArkTreeView.BranchIndentGuide class={treeViewIndentGuideBase} />
            {#each node.children ?? [] as child, index (child.value)}
              <TreeNodeView
                node={child}
                indexPath={[...indexPath, index]}
                {errors}
                {renameErrors}
                {icon}
                {selectableBranches}
                {onselect}
                {onedit}
                {oncancel}
                {contextItems}
                {onaction}
              />
            {/each}
          </ArkTreeView.BranchContent>
        </ArkTreeView.Branch>
      {:else}
        <ContextMenu items={contextItems?.(node) ?? []} onaction={action}>
          {#snippet children(triggerProps)}
            <ArkTreeView.Item
              {...triggerProps}
              class={rowClass}
              aria-label={node.description
                ? `${node.displayLabel ?? node.label} (${node.description})`
                : (node.displayLabel ?? node.label)}
              title={node.value}
              data-which-key-target=""
              data-pane-focus-target={state().selected || isDefaultFocusTarget ? '' : undefined}
              data-tree-node-value={node.value}
              data-tree-node-branch="false"
            >
              {@render nodeIcon(false)}
              <ArkTreeView.ItemText class={treeViewLabelBase} hidden={state().renaming}
                >{node.displayLabel ?? node.label}</ArkTreeView.ItemText
              >
              <ArkTreeView.NodeRenameInput
                aria-label="Name"
                data-colon-command=""
                class={treeViewRenameInputBase}
                aria-invalid={Boolean(renameErrors[node.value])}
                data-invalid={renameErrors[node.value] ? '' : undefined}
                aria-describedby={renameErrors[node.value] ? renameErrorId : undefined}
                oninput={() => onedit?.(node.value)}
                onkeydown={(event) => {
                  if (event.key === 'Escape') oncancel?.(node.value)
                }}
              />
            </ArkTreeView.Item>
          {/snippet}
        </ContextMenu>
      {/if}
      {#if renameErrors[node.value]}<p
          id={renameErrorId}
          class={treeViewMessageVariants({ variant: 'error' })}
          role="alert"
        >
          {renameErrors[node.value]}
        </p>{/if}
    {/snippet}
  </ArkTreeView.NodeContext>
</ArkTreeView.NodeProvider>
