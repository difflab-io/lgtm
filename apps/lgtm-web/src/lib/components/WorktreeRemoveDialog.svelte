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
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const project = useService('project')
</script>

{#if project.removeWorktreeConfirm}
  <Dialog
    open={true}
    onOpenChange={({ open }) => {
      if (!open) project.cancelRemoveWorktree()
    }}
  >
    <DialogBackdrop />
    <DialogContent class="max-w-md">
      <DialogHeader>
        <DialogTitle>Remove Worktree</DialogTitle>
        <DialogDescription>
          Remove worktree {project.removeWorktreeConfirm.worktreeName}?
        </DialogDescription>
      </DialogHeader>
      <div class="flex flex-col gap-2.5">
        {#if project.removeWorktreeConfirm.error}
          <Notice variant="error" class="text-xs">
            {project.removeWorktreeConfirm.error}
          </Notice>
        {/if}
        <DialogFooter>
          <Button
            variant="text"
            size="sm"
            disabled={project.removeWorktreeConfirm.submitting}
            onclick={() => project.cancelRemoveWorktree()}
          >
            Cancel
          </Button>
          <Button
            variant="solid"
            color="danger"
            size="sm"
            disabled={project.removeWorktreeConfirm.submitting}
            onclick={() =>
              void project.confirmRemoveWorktree(
                Boolean(project.removeWorktreeConfirm?.requiresForce),
              )}
          >
            {project.removeWorktreeConfirm.requiresForce ? 'Delete anyway' : 'Remove worktree'}
          </Button>
        </DialogFooter>
      </div>
      <DialogDismiss>
        <X />
      </DialogDismiss>
    </DialogContent>
  </Dialog>
{/if}
