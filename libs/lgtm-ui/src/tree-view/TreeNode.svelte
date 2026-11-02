<script lang="ts">
  import type { TreeNodeViewProps } from './types'
  import { Folder, FolderOpen, FileText } from 'lucide-svelte'
  import ContextMenu from '../context-menu/ContextMenu.svelte'
  import { TreeView as ArkTreeView, useTreeViewContext } from '@ark-ui/svelte/tree-view'
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

  // Methods -------------------------------------------------------------------
  function action(value: string) {
    // Let Ark close the menu and restore focus before starting another interaction.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (value === 'rename') tree().startRenaming(node.value)
        else onaction?.(node.value, value)
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
              >
                {@render nodeIcon(true, state().expanded)}
                <ArkTreeView.BranchText class={treeViewLabelBase} hidden={state().renaming}
                  >{node.displayLabel ?? node.label}</ArkTreeView.BranchText
                >
                <ArkTreeView.NodeRenameInput
                  aria-label="Name"
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
          <ArkTreeView.BranchContent class={treeViewBranchContentBase}>
            <ArkTreeView.BranchIndentGuide class={treeViewIndentGuideBase} />
            {#each node.children ?? [] as child, index (child.value)}
              <TreeNodeView
                node={child}
                indexPath={[...indexPath, index]}
                {errors}
                {renameErrors}
                {icon}
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
            >
              {@render nodeIcon(false)}
              <ArkTreeView.ItemText class={treeViewLabelBase} hidden={state().renaming}
                >{node.displayLabel ?? node.label}</ArkTreeView.ItemText
              >
              <ArkTreeView.NodeRenameInput
                aria-label="Name"
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
