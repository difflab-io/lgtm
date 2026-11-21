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
  import { ColorSelector } from 'lgtm-ui'
  import { X } from 'lucide-svelte'
  import { PROJECT_COLORS } from '../constants'
  import { useService } from '../services/context'
  import ProjectIconSelector from './ProjectIconSelector.svelte'

  // State ---------------------------------------------------------------------
  const project = useService('project')
</script>

{#if project.customizeProjectDialog}
  <Dialog
    open={true}
    initialFocusEl={() => document.querySelector<HTMLElement>('input[aria-label="Project name"]')}
    onOpenChange={({ open }) => {
      if (!open) project.closeCustomizeProjectDialog()
    }}
  >
    <DialogBackdrop />
    <DialogContent class="max-w-md">
      <DialogHeader>
        <DialogTitle>Edit Project</DialogTitle>
      </DialogHeader>
      <form
        class="flex flex-col gap-3"
        onsubmit={(event) => {
          event.preventDefault()
          void project.submitCustomizeProject()
        }}
      >
        <Field>
          <FieldLabel>Name</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Project name"
            value={project.customizeProjectDialog.projectName}
            oninput={(event) => {
              if (project.customizeProjectDialog) {
                project.customizeProjectDialog.projectName = event.currentTarget.value
                project.customizeProjectDialog.error = null
              }
            }}
          />
        </Field>
        <Field>
          <FieldLabel>Color</FieldLabel>
          <ColorSelector
            items={PROJECT_COLORS}
            value={project.customizeProjectDialog.color}
            onchange={(nextColor) => {
              if (project.customizeProjectDialog) {
                project.customizeProjectDialog.color = nextColor
              }
            }}
          />
        </Field>
        <Field>
          <FieldLabel>Icon</FieldLabel>
          <ProjectIconSelector
            value={project.customizeProjectDialog.icon}
            color={project.customizeProjectDialog.color}
            onchange={(nextIcon) => {
              if (project.customizeProjectDialog) {
                project.customizeProjectDialog.icon = nextIcon
              }
            }}
          />
        </Field>
        {#if project.customizeProjectDialog.error}
          <Notice variant="error" class="text-xs">
            {project.customizeProjectDialog.error}
          </Notice>
        {/if}
        <DialogFooter>
          <Button
            type="button"
            variant="text"
            size="sm"
            disabled={project.customizeProjectDialog.submitting}
            onclick={() => project.closeCustomizeProjectDialog()}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            variant="outline"
            size="sm"
            disabled={project.customizeProjectDialog.submitting}
          >
            Save
          </Button>
        </DialogFooter>
      </form>
      <DialogDismiss>
        <X />
      </DialogDismiss>
    </DialogContent>
  </Dialog>
{/if}
