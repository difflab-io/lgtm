<script lang="ts">
  import { TreeView } from 'lgtm-ui'
  import { Folder, FolderOpen, FileText } from 'lucide-svelte'
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
</script>

<TreeView
  label="Workspace files"
  version={workspace.treeVersion}
  nodes={workspace.nodes}
  renameValue={workspace.pendingEntry?.path}
  pendingNode={workspace.pendingNode}
  parentValue={workspace.pendingParent}
  refresh={workspace.refreshFolder}
  onaction={(path, action) => void workspace.contextAction(path, action)}
  contextItems={(node) => workspace.contextItems(node)}
  onrename={(path, name) => workspace.commitEntry(path, name)}
  oncancel={(value) => {
    if (value === workspace.pendingEntry?.path) workspace.pendingEntry = null
  }}
  selected={workspace.selected?.path ?? null}
  onselect={(path) => void workspace.select(path)}
  expandedValue={workspace.expandedDirs}
  onExpandedChange={(dirs) => workspace.setExpandedDirs(dirs)}
  loadChildren={(node) => workspace.loadChildren(node)}
>
  {#snippet icon(node, expanded)}
    {#if node.branch}
      {#if expanded}<FolderOpen size={13} strokeWidth={1.5} />{:else}<Folder
          size={13}
          strokeWidth={1.5}
        />{/if}
    {:else}<FileText size={13} strokeWidth={1.5} />{/if}
  {/snippet}
</TreeView>
