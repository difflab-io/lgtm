<script lang="ts">
  import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
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
    Field,
    FieldLabel,
    Input,
    ToggleGroup,
    ToggleGroupItem,
    useKeymap,
  } from '@cloudvoyant/helical-svelte'
  import { ColorSelector, TreeView, type ColorOption } from 'lgtm-ui'
  import {
    BookOpen,
    Box,
    Code,
    Compass,
    Cpu,
    Database,
    GitBranch,
    Globe,
    Layers,
    Package,
    Rocket,
    Server,
    Shield,
    Sparkles,
    Terminal,
    Wrench,
    X,
    Zap,
  } from 'lucide-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface ProjectsPaneProps {
    workspace: WorkspaceService
  }
  let { workspace }: ProjectsPaneProps = $props()

  // Constants -----------------------------------------------------------------
  const PROJECT_ICONS: { id: string; label: string; component: typeof Box }[] = [
    { id: 'box', label: 'Box', component: Box },
    { id: 'package', label: 'Package', component: Package },
    { id: 'layers', label: 'Layers', component: Layers },
    { id: 'code', label: 'Code', component: Code },
    { id: 'terminal', label: 'Terminal', component: Terminal },
    { id: 'cpu', label: 'CPU', component: Cpu },
    { id: 'database', label: 'Database', component: Database },
    { id: 'globe', label: 'Globe', component: Globe },
    { id: 'rocket', label: 'Rocket', component: Rocket },
    { id: 'server', label: 'Server', component: Server },
    { id: 'shield', label: 'Shield', component: Shield },
    { id: 'sparkles', label: 'Sparkles', component: Sparkles },
    { id: 'wrench', label: 'Wrench', component: Wrench },
    { id: 'zap', label: 'Zap', component: Zap },
    { id: 'book', label: 'Book', component: BookOpen },
    { id: 'compass', label: 'Compass', component: Compass },
  ]

  const PROJECT_COLORS: ColorOption[] = [
    { value: 'default', label: 'Default', color: null },
    { value: 'blue', label: 'Blue', color: '#3b82f6' },
    { value: 'violet', label: 'Violet', color: '#8b5cf6' },
    { value: 'emerald', label: 'Emerald', color: '#10b981' },
    { value: 'amber', label: 'Amber', color: '#f59e0b' },
    { value: 'rose', label: 'Rose', color: '#f43f5e' },
    { value: 'cyan', label: 'Cyan', color: '#06b6d4' },
    { value: 'orange', label: 'Orange', color: '#f97316' },
    { value: 'pink', label: 'Pink', color: '#ec4899' },
  ]

  // Lifecycle -----------------------------------------------------------------
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
          if (workspace.customizeProjectDialog) {
            workspace.closeCustomizeProjectDialog()
            getDocumentWhichKeyController()?.setMode('normal')
            return true
          }
          return false
        },
      },
    },
    {
      priority: 'pane',
      enabled: () =>
        Boolean(
          workspace.worktreeDialog ||
          workspace.removeWorktreeConfirm ||
          workspace.customizeProjectDialog,
        ),
    },
  )

  function resolveIconComponent(iconId?: string | null): typeof Box {
    return PROJECT_ICONS.find((entry) => entry.id === iconId)?.component ?? Box
  }

  function resolveColorHex(colorId?: string | null): string | null {
    return PROJECT_COLORS.find((entry) => entry.value === colorId)?.color ?? null
  }
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
    selected={workspace.projectsOpen ? workspace.root : null}
    onselect={(path) => void workspace.selectWorkspace(path)}
    onrename={(path, name) => workspace.renameProjectWorktree(path, name)}
    contextItems={(node) => workspace.projectContextItems(node)}
    onaction={(value, action) => void workspace.projectContextAction(value, action)}
  >
    {#snippet icon(node)}
      {@const project = workspace.projects.find((p) => p.rootPath === node.value)}
      {#if project}
        {@const IconComp = resolveIconComponent(project.icon)}
        {@const colorHex = resolveColorHex(project.color)}
        <span
          class="inline-flex items-center justify-center"
          style:color={colorHex ?? undefined}
          data-project-icon={project.icon ?? 'box'}
          data-project-color={project.color ?? 'default'}
        >
          <IconComp size={13} strokeWidth={1.6} />
        </span>
      {:else}
        <GitBranch size={13} strokeWidth={1.5} />
      {/if}
    {/snippet}
  </TreeView>
{/if}

{#if workspace.worktreeDialog}
  <Dialog
    open={true}
    initialFocusEl={() => document.querySelector<HTMLElement>('input[aria-label="Branch name"]')}
    onOpenChange={({ open }) => {
      if (!open) workspace.closeNewWorktreeDialog()
    }}
  >
    <DialogBackdrop />
    <DialogContent
      class="w-full max-w-md border border-border bg-background p-4 text-xs text-foreground shadow-lg"
    >
      <DialogHeader class="mb-3 flex flex-col gap-1 pr-6">
        <DialogTitle class="text-sm font-semibold text-foreground">New Worktree</DialogTitle>
        <DialogDescription class="text-[11px] text-muted-foreground">
          Create a managed Git worktree in {workspace.worktreeDialog.projectName}
        </DialogDescription>
      </DialogHeader>
      <form
        class="flex flex-col gap-3"
        onsubmit={(event) => {
          event.preventDefault()
          void workspace.submitNewWorktree()
        }}
      >
        <Field class="gap-1">
          <FieldLabel class="text-[11px] text-muted-foreground">Branch name</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Branch name"
            class="h-7 bg-sidebar text-xs"
            value={workspace.worktreeDialog.branch}
            oninput={(event) => {
              if (workspace.worktreeDialog) {
                workspace.worktreeDialog.branch = event.currentTarget.value
                workspace.worktreeDialog.error = null
              }
            }}
          />
        </Field>
        <Field class="gap-1">
          <FieldLabel class="text-[11px] text-muted-foreground">Base ref</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Base ref"
            class="h-7 bg-sidebar text-xs"
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
          <p class="text-[11px] leading-snug text-warn" role="alert">
            {workspace.worktreeDialog.error}
          </p>
        {/if}
        <DialogFooter class="mt-1 flex items-center justify-end gap-1.5">
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

{#if workspace.removeWorktreeConfirm}
  <Dialog
    open={true}
    onOpenChange={({ open }) => {
      if (!open) workspace.cancelRemoveWorktree()
    }}
  >
    <DialogBackdrop />
    <DialogContent
      class="w-full max-w-md border border-border bg-background p-4 text-xs text-foreground shadow-lg"
    >
      <DialogHeader class="mb-2 flex flex-col gap-1 pr-6">
        <DialogTitle class="text-sm font-semibold text-foreground">Remove Worktree</DialogTitle>
        <DialogDescription class="text-[11px] text-muted-foreground">
          Remove worktree {workspace.removeWorktreeConfirm.worktreeName}?
        </DialogDescription>
      </DialogHeader>
      <div class="flex flex-col gap-2.5">
        <p class="text-[11px] leading-snug text-muted-foreground">
          This managed worktree will be deleted from disk ({workspace.removeWorktreeConfirm
            .worktreePath}).
        </p>
        {#if workspace.removeWorktreeConfirm.error}
          <p class="text-[11px] leading-snug text-warn" role="alert">
            {workspace.removeWorktreeConfirm.error}
          </p>
        {/if}
        <DialogFooter class="mt-1 flex items-center justify-end gap-1.5">
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

{#if workspace.customizeProjectDialog}
  <Dialog
    open={true}
    initialFocusEl={() => document.querySelector<HTMLElement>('input[aria-label="Project name"]')}
    onOpenChange={({ open }) => {
      if (!open) workspace.closeCustomizeProjectDialog()
    }}
  >
    <DialogBackdrop />
    <DialogContent
      class="w-full max-w-md border border-border bg-background p-4 text-xs text-foreground shadow-lg"
    >
      <DialogHeader class="mb-3 flex flex-col gap-1 pr-6">
        <DialogTitle class="text-sm font-semibold text-foreground">Edit Project</DialogTitle>
        <DialogDescription class="text-[11px] text-muted-foreground">
          Update the project name, color, and icon
        </DialogDescription>
      </DialogHeader>
      <form
        class="flex flex-col gap-3"
        onsubmit={(event) => {
          event.preventDefault()
          void workspace.submitCustomizeProject()
        }}
      >
        <Field class="gap-1.5">
          <FieldLabel class="text-[11px] text-muted-foreground">Name</FieldLabel>
          <Input
            size="sm"
            type="text"
            data-which-key-target=""
            aria-label="Project name"
            class="h-7 bg-sidebar text-xs"
            value={workspace.customizeProjectDialog.projectName}
            oninput={(event) => {
              if (workspace.customizeProjectDialog) {
                workspace.customizeProjectDialog.projectName = event.currentTarget.value
                workspace.customizeProjectDialog.error = null
              }
            }}
          />
        </Field>
        <Field class="gap-1.5">
          <FieldLabel class="text-[11px] text-muted-foreground">Color</FieldLabel>
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
        <Field class="gap-1.5">
          <FieldLabel class="text-[11px] text-muted-foreground">Icon</FieldLabel>
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
          <p class="text-[11px] leading-snug text-warn" role="alert">
            {workspace.customizeProjectDialog.error}
          </p>
        {/if}
        <DialogFooter class="mt-1 flex items-center justify-end gap-1.5">
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
