<script lang="ts">
  import {
    Button,
    Dialog,
    DialogBackdrop,
    DialogContent,
    DialogDescription,
    DialogDismiss,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Notice,
  } from '@cloudvoyant/helical-svelte'
  import { X } from 'lucide-svelte'
  import type { WorkspaceService } from '../../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface RemoveWorktreeDialogProps {
    workspace: WorkspaceService
  }
  let { workspace }: RemoveWorktreeDialogProps = $props()
</script>

{#if workspace.removeWorktreeConfirm}
  <Dialog
    open={true}
    onOpenChange={({ open }) => {
      if (!open) workspace.cancelRemoveWorktree()
    }}
  >
    <DialogBackdrop />
    <DialogContent class="max-w-md">
      <DialogHeader>
        <DialogTitle>Remove Worktree</DialogTitle>
        <DialogDescription>
          Remove worktree {workspace.removeWorktreeConfirm.worktreeName}?
        </DialogDescription>
      </DialogHeader>
      <div class="flex flex-col gap-2.5">
        <p class="text-xs text-muted-foreground">
          This managed worktree will be deleted from disk ({workspace.removeWorktreeConfirm
            .worktreePath}).
        </p>
        {#if workspace.removeWorktreeConfirm.error}
          <Notice variant="error" class="text-xs">
            {workspace.removeWorktreeConfirm.error}
          </Notice>
        {/if}
        <DialogFooter>
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
        </DialogFooter>
      </div>
      <DialogDismiss>
        <X />
      </DialogDismiss>
    </DialogContent>
  </Dialog>
{/if}
