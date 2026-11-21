import type { Session, WorkspaceClient, WorkspaceStatePatch } from '../clients/workspaceClient'
import type { ProjectService } from './projectService.svelte'
import type { WorkspaceService } from './workspaceService.svelte'

/** Peer services orchestrated by `SessionService` when saving or restoring a session. */
export interface SessionServicePeers {
  project: ProjectService
  workspace: WorkspaceService
}

/** Frontend session orchestrator coordinating `ProjectService` and `WorkspaceService` persistence in `~/.lgtm/sessions/<id>.json`. */
export class SessionService {
  private active = true
  private sessionHydratedRoots = new Set<string>()
  private sessionSaveQueue: Promise<void> = Promise.resolve()
  private peers: SessionServicePeers | null = null

  /** Creates a `SessionService` bound to `client`. */
  constructor(private client: WorkspaceClient) {}

  // Public --------------------------------------------------------------------

  /** Binds `project` and `workspace` peer services orchestrated during session load and save. */
  bindPeers(peers: SessionServicePeers): void {
    this.peers = peers
  }

  /** Returns whether this service instance is still mounted and active. */
  get isActive(): boolean {
    return this.active
  }

  /** Returns whether persisted workspace state has already been hydrated for `root`. */
  hasHydratedRoot(root: string): boolean {
    return this.sessionHydratedRoots.has(root)
  }

  /** Marks `root` as hydrated for the current session. */
  markHydratedRoot(root: string): void {
    this.sessionHydratedRoots.add(root)
  }

  /** Clears hydration tracking and cached workspace snapshots for `root`. */
  evictWorkspace(root: string): void {
    this.sessionHydratedRoots.delete(root)
    this.peers?.workspace.evictCachedWorkspace(root)
  }

  /** Fetches the persisted `Session` from the backend, if supported by `client`. */
  async fetchSession(): Promise<Session | null> {
    if (!this.client.sessionGet) return null
    try {
      return await this.client.sessionGet()
    } catch {
      return null
    }
  }

  /** Applies an incoming `Session` snapshot across `ProjectService` and `WorkspaceService` widgets. */
  applySession(session: Session): void {
    if (!this.active || !this.peers) return
    this.peers.project.applySessionProjects(session)
    this.peers.workspace.applySessionWidgets(session)
  }

  /** Synchronizes session state from the backend and restores persisted tabs/expanded dirs for the active workspace root on first visit. */
  async syncSession(hydrateWorkspace = false): Promise<void> {
    const session = await this.fetchSession()
    const workspace = this.peers?.workspace
    if (!session || !this.active || !workspace) return
    this.applySession(session)
    if (hydrateWorkspace && !this.hasHydratedRoot(workspace.root)) {
      this.markHydratedRoot(workspace.root)
      await workspace.restoreSessionWorkspaceState(session.workspaces?.[workspace.root])
    }
  }

  /** Saves `WorkspaceService` widget/tab/explorer state via `session_save` and applies the updated `Session`. */
  async saveCurrentSession(targetRoot?: string): Promise<void> {
    const workspace = this.peers?.workspace
    if (!workspace) return
    const patch = workspace.buildSessionSavePatch(targetRoot)
    await this.saveWorkspaceState(patch)
  }

  /** Submits `patch` to `session_save` and orchestrates updating `ProjectService` and `WorkspaceService` with the saved `Session`. */
  async saveWorkspaceState(patch: WorkspaceStatePatch): Promise<void> {
    const saveFn = this.client.sessionSave
    if (!saveFn) return
    const task = this.sessionSaveQueue.then(async () => {
      try {
        const session = await saveFn(patch)
        this.applySession(session)
      } catch {
        // Ignore background session save failures.
      }
    })
    this.sessionSaveQueue = task.catch(() => {})
    await task
  }

  /** Persists the current session state and marks this service inactive on teardown. */
  dispose(): void {
    void this.saveCurrentSession()
    this.active = false
  }
}
