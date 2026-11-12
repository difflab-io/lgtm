<script lang="ts">
  import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
  import { Button, Notice, useKeymap } from '@cloudvoyant/helical-svelte'
  import { TreeView } from 'lgtm-ui'
  import { GitBranch } from 'lucide-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'
  import EditProjectDialog from './projects/EditProjectDialog.svelte'
  import NewWorktreeDialog from './projects/NewWorktreeDialog.svelte'
  import { resolveColorHex, resolveIconComponent } from './projects/projectAppearance'
  import RemoveWorktreeDialog from './projects/RemoveWorktreeDialog.svelte'

  // Props ---------------------------------------------------------------------
  interface ProjectsPaneProps {
    workspace: WorkspaceService
  }
  let { workspace }: ProjectsPaneProps = $props()

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
</script>

{#if workspace.projectNodes.length === 0}
  <div class="mx-3.5 my-2 flex flex-col items-start gap-2 text-xs text-muted-foreground">
    <Notice variant="none" class="p-0 text-xs">No projects open in this session.</Notice>
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

<NewWorktreeDialog {workspace} />
<RemoveWorktreeDialog {workspace} />
<EditProjectDialog {workspace} />
