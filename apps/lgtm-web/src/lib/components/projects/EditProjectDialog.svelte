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
    ToggleGroup,
    ToggleGroupItem,
  } from '@cloudvoyant/helical-svelte'
  import { ColorSelector } from 'lgtm-ui'
  import { X } from 'lucide-svelte'
  import type { WorkspaceService } from '../../services/workspaceService.svelte'
  import { PROJECT_COLORS, PROJECT_ICONS, resolveColorHex } from './projectAppearance'

  // Props ---------------------------------------------------------------------
  interface EditProjectDialogProps {
    workspace: WorkspaceService
  }
  let { workspace }: EditProjectDialogProps = $props()
</script>

{#if workspace.customizeProjectDialog}
  <Dialog
    open={true}
    initialFocusEl={() => document.querySelector<HTMLElement>('input[aria-label="Project name"]')}
    onOpenChange={({ open }) => {
      if (!open) workspace.closeCustomizeProjectDialog()
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
          void workspace.submitCustomizeProject()
        }}
      >
        <Field>
          <FieldLabel>Name</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Project name"
            value={workspace.customizeProjectDialog.projectName}
            oninput={(event) => {
              if (workspace.customizeProjectDialog) {
                workspace.customizeProjectDialog.projectName = event.currentTarget.value
                workspace.customizeProjectDialog.error = null
              }
            }}
          />
        </Field>
        <Field>
          <FieldLabel>Color</FieldLabel>
          <ColorSelector
            items={PROJECT_COLORS}
            value={workspace.customizeProjectDialog.color}
            onchange={(nextColor) => {
              if (workspace.customizeProjectDialog) {
                workspace.customizeProjectDialog.color = nextColor
              }
            }}
          />
        </Field>
        <Field>
          <FieldLabel>Icon</FieldLabel>
          <ToggleGroup
            value={[workspace.customizeProjectDialog.icon]}
            multiple={false}
            aria-label="Project icon"
            class="grid grid-cols-8 gap-1"
            onValueChange={({ value: next }) => {
              if (workspace.customizeProjectDialog && next[0]) {
                workspace.customizeProjectDialog.icon = next[0]
              }
            }}
          >
            {#each PROJECT_ICONS as item (item.id)}
              {@const IconComp = item.component}
              {@const active = workspace.customizeProjectDialog.icon === item.id}
              {@const previewHex = resolveColorHex(workspace.customizeProjectDialog.color)}
              <ToggleGroupItem
                value={item.id}
                variant="outline"
                size="sm"
                aria-label="Icon {item.label}"
                title={item.label}
                style={active && previewHex ? `color: ${previewHex}` : undefined}
                class="size-7 p-0"
              >
                <IconComp size={14} strokeWidth={1.6} />
              </ToggleGroupItem>
            {/each}
          </ToggleGroup>
        </Field>
        {#if workspace.customizeProjectDialog.error}
          <Notice variant="error" class="text-xs">
            {workspace.customizeProjectDialog.error}
          </Notice>
        {/if}
        <DialogFooter>
          <Button
            type="button"
            variant="text"
            size="sm"
            disabled={workspace.customizeProjectDialog.submitting}
            onclick={() => workspace.closeCustomizeProjectDialog()}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            variant="outline"
            size="sm"
            disabled={workspace.customizeProjectDialog.submitting}
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
