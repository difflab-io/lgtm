<script lang="ts">
  import { Button, Row, Sidebar, SidebarContent } from '@cloudvoyant/helical-svelte'
  import { FolderPlus, GitBranch } from 'lucide-svelte'
  import { useService } from '../services/context'
  import ProjectsPane from './ProjectsPane.svelte'
  import SidebarPaneHeader from './SidebarPaneHeader.svelte'
  import WidgetPane from './WidgetPane.svelte'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const project = useService('project')
  const session = useService('session')
  const dockOpen = $derived(session.projectsOpen || session.explorerOpen)

  function startResize(target: 'projects' | 'widget', event: PointerEvent) {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = target === 'projects' ? session.projectsWidth : session.widgetWidth
    session.setResizing(true)

    function onMove(moveEvent: PointerEvent) {
      const delta = moveEvent.clientX - startX
      if (target === 'projects') {
        session.setProjectsWidth(startWidth + delta)
      } else {
        session.setWidgetWidth(startWidth + delta)
      }
    }

    function onUp() {
      session.setResizing(false)
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
    }

    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }
</script>

<Sidebar
  id="workspace-sidebar"
  collapsible="none"
  class={[
    'workspace-sidebar h-full min-h-0 w-auto shrink-0 overflow-hidden bg-sidebar outline-none',
    dockOpen ? 'border-r border-border' : '',
  ].join(' ')}
  aria-label="Workspace"
>
  <Row class="min-h-0 flex-1 items-stretch gap-0 overflow-hidden">
    <div
      style="width: {session.projectsOpen ? `${session.projectsWidth}px` : '0px'}"
      class={[
        'relative h-full min-h-0 shrink-0 overflow-hidden',
        !session.resizing ? 'transition-[width] duration-200 ease-linear' : '',
        session.projectsOpen && session.explorerOpen ? 'border-r border-border' : '',
      ].join(' ')}
    >
      <Sidebar
        id="projects-dock-pane"
        data-testid="projects-dock-pane"
        paneId="projects"
        paneTitle="Projects"
        paneOrder={1}
        paneDisabled={!session.projectsOpen &&
          !project.worktreeDialog &&
          !project.newBranchDialog &&
          !project.removeWorktreeConfirm &&
          !project.customizeProjectDialog}
        collapsible="none"
        aria-hidden={!session.projectsOpen}
        inert={!session.projectsOpen &&
        !project.worktreeDialog &&
        !project.newBranchDialog &&
        !project.removeWorktreeConfirm &&
        !project.customizeProjectDialog
          ? true
          : undefined}
        style="width: {session.projectsWidth}px"
        class={[
          'h-full min-h-0 shrink-0 overflow-hidden bg-sidebar outline-none',
          workspace.settings.paneOutline &&
            'data-[pane-active=true]:ring-1 data-[pane-active=true]:ring-ring/40 data-[pane-active=true]:ring-inset',
        ]
          .filter(Boolean)
          .join(' ')}
        aria-label="Projects"
      >
        <SidebarPaneHeader title="Projects" class="px-3">
          {#snippet actions()}
            <Button
              variant="text"
              size="sm"
              class="size-6 p-1 text-[10px] font-medium"
              aria-label="Add project"
              title="Add project"
              disabled={workspace.blocked}
              onclick={() => void workspace.selectWorkspace('__open_folder__')}
              ><FolderPlus size={14} strokeWidth={1.5} /></Button
            >
            <Button
              variant="text"
              size="sm"
              class="size-6 p-1 text-[10px] font-medium"
              aria-label="New worktree"
              title="New worktree"
              disabled={!project.activeProject || workspace.blocked}
              onclick={() => project.openNewWorktreeDialog()}
              ><GitBranch size={14} strokeWidth={1.5} /></Button
            >
          {/snippet}
        </SidebarPaneHeader>
        <SidebarContent class="min-h-0 flex-1 overflow-y-auto">
          <ProjectsPane />
        </SidebarContent>
      </Sidebar>
      {#if session.projectsOpen}
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize Projects pane"
          data-testid="projects-pane-resizer"
          class="absolute top-0 right-0 z-10 h-full w-1 cursor-col-resize hover:bg-ring/40"
          onpointerdown={(event) => startResize('projects', event)}
        ></div>
      {/if}
    </div>

    <div
      style="width: {session.explorerOpen ? `${session.widgetWidth}px` : '0px'}"
      class={[
        'relative h-full min-h-0 shrink-0 overflow-hidden',
        !session.resizing ? 'transition-[width] duration-200 ease-linear' : '',
      ].join(' ')}
    >
      <WidgetPane />
      {#if session.explorerOpen}
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize Explorer pane"
          data-testid="widget-pane-resizer"
          class="absolute top-0 right-0 z-10 h-full w-1 cursor-col-resize hover:bg-ring/40"
          onpointerdown={(event) => startResize('widget', event)}
        ></div>
      {/if}
    </div>
  </Row>
</Sidebar>
