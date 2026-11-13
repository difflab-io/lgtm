<script lang="ts">
  import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
  import { Button, Notice, useKeymap } from '@cloudvoyant/helical-svelte'
  import { TreeView } from 'lgtm-ui'
  import { GitBranch } from 'lucide-svelte'
  import { resolveColorHex, resolveIconComponent } from '../constants'
  import { useService } from '../services/context'
  import BranchNewDialog from './BranchNewDialog.svelte'
  import ProjectEditDialog from './ProjectEditDialog.svelte'
  import WorktreeNewDialog from './WorktreeNewDialog.svelte'
  import WorktreeRemoveDialog from './WorktreeRemoveDialog.svelte'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const project = useService('project')
  const session = useService('session')

  // Lifecycle -----------------------------------------------------------------
  useKeymap(
    {
      Escape: {
        key: 'Escape',
        label: 'Close Dialog',
        mode: ['normal', 'select', 'insert'],
        hidden: true,
        action: () => {
          if (project.worktreeDialog) {
            project.closeNewWorktreeDialog()
            getDocumentWhichKeyController()?.setMode('normal')
            return true
          }
          if (project.newBranchDialog) {
            project.closeNewBranchDialog()
            getDocumentWhichKeyController()?.setMode('normal')
            return true
          }
          if (project.removeWorktreeConfirm) {
            project.cancelRemoveWorktree()
            getDocumentWhichKeyController()?.setMode('normal')
            return true
          }
          if (project.customizeProjectDialog) {
            project.closeCustomizeProjectDialog()
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
          project.worktreeDialog ||
          project.newBranchDialog ||
          project.removeWorktreeConfirm ||
          project.customizeProjectDialog,
        ),
    },
  )
</script>

{#if project.projectNodes.length === 0}
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
    nodes={project.projectNodes}
    expandedValue={project.projectExpandedValues}
    onExpandedChange={(next) => void project.handleProjectExpandedChange(next)}
    selected={session.projectsOpen ? workspace.root : null}
    onselect={(path) => void workspace.selectWorkspace(path)}
    onrename={(path, name) => project.renameProjectWorktree(path, name)}
    contextItems={(node) => project.projectContextItems(node)}
    onaction={(value, action) => void project.projectContextAction(value, action)}
  >
    {#snippet icon(node)}
      {@const ownerProject = project.projects.find((p) => p.rootPath === node.value)}
      {#if ownerProject}
        {@const IconComp = resolveIconComponent(ownerProject.icon)}
        {@const colorHex = resolveColorHex(ownerProject.color)}
        <span
          class="inline-flex items-center justify-center"
          style:color={colorHex ?? undefined}
          data-project-icon={ownerProject.icon ?? 'box'}
          data-project-color={ownerProject.color ?? 'default'}
        >
          <IconComp size={13} strokeWidth={1.6} />
        </span>
      {:else}
        <GitBranch size={13} strokeWidth={1.5} />
      {/if}
    {/snippet}
  </TreeView>
{/if}

<WorktreeNewDialog />
<BranchNewDialog />
<WorktreeRemoveDialog />
<ProjectEditDialog />
