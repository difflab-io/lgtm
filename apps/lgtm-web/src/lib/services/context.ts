import { getContext, setContext } from 'svelte'
import type { LspService } from './lspService.svelte'
import type { ProjectService } from './projectService.svelte'
import type { SessionService } from './sessionService.svelte'
import type { SettingsService } from './settingsService.svelte'
import type { WorkspaceService } from './workspaceService.svelte'

const SERVICES_CONTEXT_KEY = Symbol('lgtm.services')

/** Application domain service registry accessible via `useService`. */
export interface AppServices {
  workspace: WorkspaceService
  project: ProjectService
  session: SessionService
  lsp: LspService
  settings: SettingsService
}

// Public ----------------------------------------------------------------------

/** Registers all domain services (`workspace`, `project`, `session`, `lsp`, `settings`) in the component tree's Svelte context. */
export function setServicesContext(workspace: WorkspaceService): AppServices {
  const services: AppServices = {
    workspace,
    project: workspace.project,
    session: workspace.session,
    lsp: workspace.lsp,
    settings: workspace.settings,
  }
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
