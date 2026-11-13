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
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const project = useService('project')
</script>

{#if project.worktreeDialog}
  <Dialog
    open={true}
    initialFocusEl={() => document.querySelector<HTMLElement>('input[aria-label="Branch name"]')}
    onOpenChange={({ open }) => {
      if (!open) project.closeNewWorktreeDialog()
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
          void project.submitNewWorktree()
        }}
      >
        <Field>
          <FieldLabel>Branch name</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Branch name"
            value={project.worktreeDialog.branch}
            oninput={(event) => {
              project.updateWorktreeDialogBranch(event.currentTarget.value)
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
            value={project.worktreeDialog.baseRef}
            oninput={(event) => {
              if (project.worktreeDialog) {
                project.worktreeDialog.baseRef = event.currentTarget.value
              }
            }}
          />
        </Field>
        <p
          class="truncate text-[10px] text-muted-foreground"
          data-testid="worktree-preview-path"
          title={project.worktreePreviewPath}
        >
          {project.worktreePreviewPath}
        </p>
        {#if project.worktreeDialog.error}
          <Notice variant="error" class="text-xs">
            {project.worktreeDialog.error}
          </Notice>
        {/if}
        <DialogFooter>
          <Button
            type="button"
            variant="text"
            size="sm"
            disabled={project.worktreeDialog.submitting}
            onclick={() => project.closeNewWorktreeDialog()}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            variant="outline"
            size="sm"
            disabled={project.worktreeDialog.submitting}
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
