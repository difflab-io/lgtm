<script lang="ts">
  import {
    Button,
    Dialog,
    DialogBackdrop,
    DialogContent,
    DialogDismiss,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Field,
    FieldLabel,
    Input,
    Notice,
  } from '@cloudvoyant/helical-svelte'
  import { X } from 'lucide-svelte'
  import type { WorkspaceService } from '../../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface NewWorktreeDialogProps {
    workspace: WorkspaceService
  }
  let { workspace }: NewWorktreeDialogProps = $props()
</script>

{#if workspace.worktreeDialog}
  <Dialog
    open={true}
    initialFocusEl={() => document.querySelector<HTMLElement>('input[aria-label="Branch name"]')}
    onOpenChange={({ open }) => {
      if (!open) workspace.closeNewWorktreeDialog()
    }}
  >
    <DialogBackdrop />
    <DialogContent class="max-w-md">
      <DialogHeader>
        <DialogTitle>New Worktree</DialogTitle>
      </DialogHeader>
      <form
        class="flex flex-col gap-3"
        onsubmit={(event) => {
          event.preventDefault()
          void workspace.submitNewWorktree()
        }}
      >
        <Field>
          <FieldLabel>Branch name</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Branch name"
            value={workspace.worktreeDialog.branch}
            oninput={(event) => {
              if (workspace.worktreeDialog) {
                workspace.worktreeDialog.branch = event.currentTarget.value
                workspace.worktreeDialog.error = null
              }
            }}
          />
        </Field>
        <Field>
          <FieldLabel>Base ref</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Base ref"
            value={workspace.worktreeDialog.baseRef}
            oninput={(event) => {
              if (workspace.worktreeDialog) {
                workspace.worktreeDialog.baseRef = event.currentTarget.value
              }
            }}
          />
        </Field>
        <p
          class="truncate text-[10px] text-muted-foreground"
          data-testid="worktree-preview-path"
          title={workspace.worktreePreviewPath}
        >
          {workspace.worktreePreviewPath}
        </p>
        {#if workspace.worktreeDialog.error}
          <Notice variant="error" class="text-xs">
            {workspace.worktreeDialog.error}
          </Notice>
        {/if}
        <DialogFooter>
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
        </DialogFooter>
      </form>
      <DialogDismiss>
        <X />
      </DialogDismiss>
    </DialogContent>
  </Dialog>
{/if}
