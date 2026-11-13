/** Modal state for the `New Worktree` creation dialog (`WorktreeNewDialog.svelte`). */
export interface WorktreeDialogState {
  projectId: string
  projectName: string
  branch: string
  baseRef: string
  submitting: boolean
  error: string | null
}

/** Modal state for the `New Branch` creation dialog (`BranchNewDialog.svelte`). */
export interface NewBranchDialogState {
  projectId: string
  workspaceRoot: string
  branch: string
  submitting: boolean
  error: string | null
}

/** Modal state for the `Remove Worktree` confirmation dialog (`WorktreeRemoveDialog.svelte`). */
export interface RemoveWorktreeConfirmState {
  projectId: string
  projectName: string
  worktreePath: string
  worktreeName: string
  requiresForce: boolean
  submitting: boolean
  error: string | null
}

/** Modal state for the `Edit Project` appearance dialog (`ProjectEditDialog.svelte`). */
export interface CustomizeProjectDialogState {
  projectId: string
  projectName: string
  icon: string
  color: string
  submitting: boolean
  error: string | null
}

// Public ----------------------------------------------------------------------

/** Validates a candidate Git branch name against ref syntax rules and checked-out/existing branch sets. */
export function validateGitBranchName(
  rawBranch: string,
  options?: {
    checkedOutBranches?: Iterable<string>
    existingBranches?: Iterable<string>
    requireNew?: boolean
  },
): string | null {
  const branch = rawBranch.trim()
  if (!branch) {
    return 'Branch name cannot be empty.'
  }
  if (
    branch === '@' ||
    branch.startsWith('/') ||
    branch.endsWith('/') ||
    branch.startsWith('.') ||
    branch.endsWith('.') ||
    branch.endsWith('.lock') ||
    branch.includes('..') ||
    branch.includes('@{') ||
    branch.includes('//') ||
    // eslint-disable-next-line no-control-regex
    /[\s~^:?*[\\\x00-\x1f\x7f]/.test(branch) ||
    branch
      .split('/')
      .some((segment) => !segment || segment.startsWith('.') || segment.endsWith('.lock'))
  ) {
    return `Invalid branch name '${branch}'.`
  }
  if (options?.checkedOutBranches) {
    for (const active of options.checkedOutBranches) {
      if (active === branch) {
        return `Branch '${branch}' is already checked out in another worktree.`
      }
    }
  }
  if (options?.requireNew && options.existingBranches) {
    for (const existing of options.existingBranches) {
      if (existing === branch) {
        return `Branch '${branch}' already exists.`
      }
    }
  }
  return null
}
