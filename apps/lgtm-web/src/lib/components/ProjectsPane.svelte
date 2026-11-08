<script lang="ts">
  import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
  import { Button, useKeymap } from '@cloudvoyant/helical-svelte'
  import { TreeView } from 'lgtm-ui'
  import { Folder, FolderOpen, GitBranch } from 'lucide-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface ProjectsPaneProps {
    workspace: WorkspaceService
  }
  let { workspace }: ProjectsPaneProps = $props()

  // State ---------------------------------------------------------------------
  let branchInputEl = $state<HTMLInputElement | null>(null)

  $effect(() => {
    if (workspace.worktreeDialog && branchInputEl) {
      branchInputEl.focus()
    }
  })

  useKeymap(
    {
      Escape: {
        key: 'Escape',
        label: 'Close Dialog',
        mode: ['normal', 'select', 'insert'],
        hidden: true,
        action: () => {
          if (workspace.worktreeDialog) {
            workspace.closeNewWorktreeDialog()
            getDocumentWhichKeyController()?.setMode('normal')
            return true
          }
          if (workspace.removeWorktreeConfirm) {
            workspace.cancelRemoveWorktree()
            getDocumentWhichKeyController()?.setMode('normal')
            return true
          }
          return false
        },
      },
    },
    {
      priority: 'pane',
      enabled: () => Boolean(workspace.worktreeDialog || workspace.removeWorktreeConfirm),
    },
  )
</script>

{#if workspace.projectNodes.length === 0}
  <div class="mx-3.5 my-2 flex flex-col items-start gap-2 text-xs text-muted-foreground">
    <p class="leading-relaxed">No projects open in this session.</p>
    <Button
      variant="outline"
      size="sm"
      onclick={() => void workspace.selectWorkspace('__open_folder__')}
    >
      Add project
    </Button>
  </div>
{:else}
  <TreeView
    label="Projects"
    selectableBranches={true}
    nodes={workspace.projectNodes}
    expandedValue={workspace.projectExpandedValues}
    onExpandedChange={(next) => void workspace.handleProjectExpandedChange(next)}
    selected={workspace.root}
    onselect={(path) => void workspace.selectWorkspace(path)}
    contextItems={(node) => workspace.projectContextItems(node)}
    onaction={(value, action) => void workspace.projectContextAction(value, action)}
  >
    {#snippet icon(node, expanded)}
      {#if workspace.projects.some((p) => p.rootPath === node.value)}
        {#if expanded}<FolderOpen size={13} strokeWidth={1.5} />{:else}<Folder
            size={13}
            strokeWidth={1.5}
          />{/if}
      {:else}
        <GitBranch size={13} strokeWidth={1.5} />
      {/if}
    {/snippet}
  </TreeView>
{/if}

{#if workspace.worktreeDialog}
  <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
  <form
    class="mx-2.5 my-2 flex flex-col gap-2 border border-border bg-background p-2.5 text-xs text-foreground"
    role="dialog"
    aria-label="New Worktree"
    onsubmit={(event) => {
      event.preventDefault()
      void workspace.submitNewWorktree()
    }}
  >
    <div class="font-semibold">New worktree in {workspace.worktreeDialog.projectName}</div>
    <label class="flex flex-col gap-1 text-[11px] text-muted-foreground">
      <span>Branch name</span>
      <input
        bind:this={branchInputEl}
        type="text"
        data-which-key-target=""
        aria-label="Branch name"
        class="h-6 border border-border bg-sidebar px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-ring"
        value={workspace.worktreeDialog.branch}
        oninput={(event) => {
          if (workspace.worktreeDialog) {
            workspace.worktreeDialog.branch = event.currentTarget.value
            workspace.worktreeDialog.error = null
          }
        }}
      />
    </label>
    <label class="flex flex-col gap-1 text-[11px] text-muted-foreground">
      <span>Base ref</span>
      <input
        type="text"
        data-which-key-target=""
        aria-label="Base ref"
        class="h-6 border border-border bg-sidebar px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-ring"
        value={workspace.worktreeDialog.baseRef}
        oninput={(event) => {
          if (workspace.worktreeDialog) {
            workspace.worktreeDialog.baseRef = event.currentTarget.value
          }
        }}
      />
    </label>
    <p
      class="truncate text-[10px] text-muted-foreground"
      data-testid="worktree-preview-path"
      title={workspace.worktreePreviewPath}
    >
      {workspace.worktreePreviewPath}
    </p>
    {#if workspace.worktreeDialog.error}
      <p class="text-[11px] leading-snug text-warn" role="alert">
        {workspace.worktreeDialog.error}
      </p>
    {/if}
    <div class="flex items-center justify-end gap-1.5 pt-1">
      <Button
        type="button"
        variant="text"
        size="sm"
        disabled={workspace.worktreeDialog.submitting}
        onclick={() => workspace.closeNewWorktreeDialog()}
      >
        Cancel
      </Button>
      <Button
        type="submit"
        variant="outline"
        size="sm"
        disabled={workspace.worktreeDialog.submitting}
      >
        Create worktree
      </Button>
    </div>
  </form>
{/if}

{#if workspace.removeWorktreeConfirm}
  <div
    class="mx-2.5 my-2 flex flex-col gap-2 border border-border bg-background p-2.5 text-xs text-foreground"
    role="dialog"
    aria-label="Remove Worktree"
  >
    <div class="font-semibold">Remove worktree {workspace.removeWorktreeConfirm.worktreeName}?</div>
    {#if workspace.removeWorktreeConfirm.source === 'managed'}
      <p class="text-[11px] leading-snug text-muted-foreground">
        This managed worktree will be deleted from disk ({workspace.removeWorktreeConfirm
          .worktreePath}).
      </p>
    {:else}
      <p class="text-[11px] leading-snug text-muted-foreground">
        This external worktree will be detached from the session without deleting files on disk.
      </p>
    {/if}
    {#if workspace.removeWorktreeConfirm.error}
      <p class="text-[11px] leading-snug text-warn" role="alert">
        {workspace.removeWorktreeConfirm.error}
      </p>
    {/if}
    <div class="flex items-center justify-end gap-1.5 pt-1">
      <Button
        variant="text"
        size="sm"
        disabled={workspace.removeWorktreeConfirm.submitting}
        onclick={() => workspace.cancelRemoveWorktree()}
      >
        Cancel
      </Button>
      <Button
        variant="outline"
        size="sm"
        disabled={workspace.removeWorktreeConfirm.submitting}
        onclick={() =>
          void workspace.confirmRemoveWorktree(
            Boolean(workspace.removeWorktreeConfirm?.requiresForce),
          )}
      >
        {workspace.removeWorktreeConfirm.requiresForce ? 'Delete anyway' : 'Remove worktree'}
      </Button>
    </div>
  </div>
{/if}
