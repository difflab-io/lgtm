import type { DiffWorkspaceSnapshot, ReviewMode, WorkspaceClient } from '../clients/workspaceClient'

/** Reactive frontend service tracking the active review mode (`local` vs. `forge_backed`). */
export class ReviewService {
  /** Active review mode (`local` by default). */
  mode = $state<ReviewMode>('local')

  /** Constructs a `ReviewService` bound to `client` and an optional snapshot callback. */
  constructor(
    private client: WorkspaceClient,
    private onSnapshot?: (snapshot: DiffWorkspaceSnapshot) => void,
  ) {}

  /** Synchronizes the local reactive mode from a backend snapshot without issuing an IPC call. */
  syncMode(mode: ReviewMode): void {
    this.mode = mode
  }

  /** Switches the backend review mode to `nextMode` and forwards the refreshed diff snapshot. */
  async setMode(nextMode: ReviewMode): Promise<DiffWorkspaceSnapshot | null> {
    this.mode = nextMode
    if (!this.client.isDesktop() || !this.client.reviewSetMode) {
      return null
    }
    const snapshot = await this.client.reviewSetMode(nextMode)
    this.mode = snapshot.reviewMode
    this.onSnapshot?.(snapshot)
    return snapshot
  }

  /** Toggles between `local` and `forge_backed` review modes. */
  async toggleMode(): Promise<DiffWorkspaceSnapshot | null> {
    const nextMode: ReviewMode = this.mode === 'local' ? 'forge_backed' : 'local'
    return this.setMode(nextMode)
  }
}
