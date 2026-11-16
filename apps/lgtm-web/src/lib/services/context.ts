import { getContext, setContext } from 'svelte'
import type { WorkspaceClient } from '../clients/workspaceClient'
import { HotkeyService } from './hotkeyService.svelte'
import { LspService } from './lspService.svelte'
import { ProjectService } from './projectService.svelte'
import { SessionService } from './sessionService.svelte'
import { SettingsService } from './settingsService.svelte'
import { WorkspaceService } from './workspaceService.svelte'

const SERVICES_CONTEXT_KEY = Symbol('lgtm.services')

/** Application domain service registry accessible via `useService`. */
export interface AppServices {
  workspace: WorkspaceService
  project: ProjectService
  session: SessionService
  lsp: LspService
  settings: SettingsService
  hotkeys?: HotkeyService
}

// Public ----------------------------------------------------------------------

/** Constructs peer frontend domain services wired 1:1 to their backend IPC services (`lgtm-session`, `lgtm-project`, `lgtm-lsp`, `lgtm-config`, and `lgtm-workspace`). */
export function createAppServices(client: WorkspaceClient): Required<AppServices> {
  const settings = new SettingsService(client)
  const lsp = new LspService(client)
  const session = new SessionService(client)
  const workspace = new WorkspaceService(client, session, lsp, settings)
  const project = new ProjectService(client, {
    getRoot: () => session.root,
    isActive: () => session.isActive,
    applySession: (nextSession) => session.applySession(nextSession),
    openWorkspace: (path) => session.openWorkspace(path),
    selectWorkspace: (path) => session.selectWorkspace(path),
    evictWorkspace: (path) => session.evictWorkspace(path),
    setCreationError: (message) => {
      workspace.creationError = message
    },
  })
  session.bindPeers({ workspace, project, lsp, settings })
  const services: AppServices = { workspace, project, session, lsp, settings }
  const hotkeys = new HotkeyService(services)
  return { workspace, project, session, lsp, settings, hotkeys }
}

/** Registers all domain services (`workspace`, `project`, `session`, `lsp`, `settings`, `hotkeys`) in the component tree's Svelte context. */
export function setServicesContext(services: AppServices): AppServices {
  return setContext(SERVICES_CONTEXT_KEY, services)
}

/** Retrieves a domain service (`'workspace'`, `'project'`, `'session'`, `'lsp'`, or `'settings'`) from Svelte context. */
export function useService<K extends keyof AppServices>(key: K): AppServices[K] {
  const services = getContext<AppServices | undefined>(SERVICES_CONTEXT_KEY)
  if (!services) {
    throw new Error(
      `useService('${key}') must be called inside a component tree initialized with setServicesContext.`,
    )
  }
  return services[key]
}
