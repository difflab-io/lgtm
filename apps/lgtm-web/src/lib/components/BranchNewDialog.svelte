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

{#if project.newBranchDialog}
  <Dialog
    open={true}
    initialFocusEl={() => document.querySelector<HTMLElement>('input[aria-label="Branch name"]')}
    onOpenChange={({ open }) => {
      if (!open) project.closeNewBranchDialog()
    }}
  >
    <DialogBackdrop />
    <DialogContent class="max-w-sm">
      <DialogHeader>
        <DialogTitle>New Branch</DialogTitle>
      </DialogHeader>
      <form
        class="flex flex-col gap-3"
        onsubmit={(event) => {
          event.preventDefault()
          void project.submitNewBranch()
        }}
      >
        <Field>
          <FieldLabel>Branch name</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Branch name"
            value={project.newBranchDialog.branch}
            oninput={(event) => {
              project.updateNewBranchDialogBranch(event.currentTarget.value)
            }}
          />
        </Field>
        {#if project.newBranchDialog.error}
          <Notice variant="error" class="text-xs">
            {project.newBranchDialog.error}
          </Notice>
        {/if}
        <DialogFooter>
          <Button
            type="button"
            variant="text"
            size="sm"
            disabled={project.newBranchDialog.submitting}
            onclick={() => project.closeNewBranchDialog()}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            variant="outline"
            size="sm"
            disabled={project.newBranchDialog.submitting}
          >
            Create branch
          </Button>
        </DialogFooter>
      </form>
      <DialogDismiss>
        <X />
      </DialogDismiss>
    </DialogContent>
  </Dialog>
{/if}
