import type { TreeNode } from 'lgtm-ui'
import type { WorkspaceNode } from '../clients/workspaceClient'

/** Display mapping for a workspace entry without filesystem access. */
export class WorkspaceNodeViewModel {
  constructor(readonly entry: WorkspaceNode) {}

  toTreeNode(): TreeNode {
    return {
      value: this.entry.path,
      label: this.entry.name,
      description: this.entry.kind === 'directory' ? undefined : this.entry.name,
      branch: this.entry.kind === 'directory',
    }
  }
}
