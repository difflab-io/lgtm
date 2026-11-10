import { tick } from 'svelte'
import {
  getDocumentWhichKeyController,
  type CommandNode,
  type CommandStyle as HelicalCommandStyle,
  type KeymapHandler,
  type KeymapMode,
  type WhichKeyNode,
} from '@cloudvoyant/helical-ui'
import type { WorkspaceService } from './workspaceService.svelte'

function leaf(
  key: string,
  label: string,
  category: string,
  commandId: string,
  description: string,
  mode?: KeymapMode[],
): WhichKeyNode {
  return { key, label, category, commandId, description, ...(mode ? { mode } : {}) }
}

/**
 * Application-level modal keybinding, `WhichKey` leader menu, and `CommandMenu`/`CommandPalette` registry.
 */
export class HotkeyService {
  private readonly workspace: WorkspaceService

  /** Whether the left sidebar is currently expanded. */
  sidebarOpen = $state(true)
  /** Whether the active command surface (`CommandMenu` or `CommandPalette`) is open. */
  commandOpen = $state(false)
  /** Transient override forcing `CommandMenu` (`colon`) when opened via `:`. */
  commandStyleOverride = $state<'colon' | null>(null)

  /** Constructs a `HotkeyService` bound to `workspace`. */
  constructor(workspace: WorkspaceService) {
    this.workspace = workspace
  }

  /** Effective Helical command surface style (`menu` or `colon`). */
  get effectiveCommandStyle(): HelicalCommandStyle {
    return this.commandStyleOverride ?? 'menu'
  }

  /** Application keymap merged above the active modal preset. */
  readonly keymap: Record<string, WhichKeyNode> = {
    'Mod+s': leaf(
      'Mod+s',
      'Save File',
      'File',
      'file.save',
      'Flush pending changes and save the active file',
      ['normal', 'select', 'insert'],
    ),
    'Mod+k': leaf(
      'Mod+k',
      'Command Surface',
      'Command',
      'command.openConfigured',
      'Open the CommandMenu',
      ['normal', 'select', 'insert'],
    ),
    'Mod+b': leaf(
      'Mod+b',
      'Toggle Explorer Dock',
      'Workspace',
      'workspace.toggleExplorer',
      'Toggle the widget dock pane open or closed',
      ['normal', 'select', 'insert'],
    ),
    'Mod+Shift+b': leaf(
      'Mod+Shift+b',
      'Toggle Projects Dock',
      'Projects',
      'projects.show',
      'Toggle the Projects dock pane open or closed',
      ['normal', 'select', 'insert'],
    ),
    'Mod+Shift+B': leaf(
      'Mod+Shift+B',
      'Toggle Projects Dock',
      'Projects',
      'projects.show',
      'Toggle the Projects dock pane open or closed',
      ['normal', 'select', 'insert'],
    ),
    'Mod+,': leaf(
      'Mod+,',
      'Global Settings',
      'Preferences',
      'settings.openGlobal',
      'Open ~/.lgtm/config.json in the editor',
      ['normal', 'select', 'insert'],
    ),
    'Mod+Shift+,': leaf(
      'Mod+Shift+,',
      'Project Settings',
      'Preferences',
      'settings.openProject',
      'Open .lgtm/config.json in the editor',
      ['normal', 'select', 'insert'],
    ),
    'Mod+Shift+<': leaf(
      'Mod+Shift+<',
      'Project Settings',
      'Preferences',
      'settings.openProject',
      'Open .lgtm/config.json in the editor',
      ['normal', 'select', 'insert'],
    ),
    'Mod+<': leaf(
      'Mod+<',
      'Project Settings',
      'Preferences',
      'settings.openProject',
      'Open .lgtm/config.json in the editor',
      ['normal', 'select', 'insert'],
    ),
    ':': leaf(
      ':',
      'Command Menu',
      'Command',
      'command.openColon',
      'Open the bottom-docked colon CommandMenu (:)',
      ['normal', 'select'],
    ),
    Space: {
      key: 'Space',
      label: '+leader',
      group: 'Leader',
      category: 'Leader',
      mode: ['normal', 'select'],
      children: {
        '?': leaf(
          '?',
          'Keybind Help',
          'Leader',
          'whichKey.openExplorer',
          'Open searchable keybinding explorer',
        ),
        ':': leaf(
          ':',
          'Command Surface',
          'Command',
          'command.openConfigured',
          'Open the CommandMenu',
        ),
        e: leaf(
          'e',
          'Toggle Explorer Dock',
          'Workspace',
          'workspace.toggleExplorer',
          'Toggle the widget dock pane open or closed',
        ),
        p: {
          key: 'p',
          label: '+projects',
          group: 'Projects',
          description: 'Manage projects and Git worktrees',
          category: 'Projects',
          children: {
            p: leaf(
              'p',
              'Toggle Projects Dock',
              'Projects',
              'projects.show',
              'Toggle the Projects dock pane open or closed',
            ),
            e: leaf(
              'e',
              'Toggle Explorer Dock',
              'Projects',
              'projects.showExplorer',
              'Toggle the widget dock pane open or closed',
            ),
            a: leaf(
              'a',
              'Add Project',
              'Projects',
              'projects.add',
              'Open a project folder and attach it to the session',
            ),
            n: leaf(
              'n',
              'New Worktree',
              'Projects',
              'projects.newWorktree',
              'Create a managed Git worktree for the active project',
            ),
            d: leaf(
              'd',
              'Remove Worktree',
              'Projects',
              'projects.removeWorktree',
              'Remove the active linked Git worktree',
            ),
          },
        },
        f: {
          key: 'f',
          label: '+file',
          group: 'File',
          description: 'File and workspace operations',
          category: 'File',
          children: {
            s: leaf('s', 'Save File', 'File', 'file.save', 'Save the active file'),
            n: leaf('n', 'New File', 'File', 'file.new', 'Create a new file in the workspace'),
            N: leaf(
              'N',
              'New Folder',
              'File',
              'file.newFolder',
              'Create a new folder in the workspace',
            ),
            o: leaf(
              'o',
              'Open Workspace Folder',
              'Workspace',
              'workspace.openFolder',
              'Open a workspace folder',
            ),
            c: leaf(
              'c',
              'Global Settings (JSON)',
              'Settings',
              'settings.openGlobal',
              'Open ~/.lgtm/config.json in the editor',
            ),
            p: leaf(
              'p',
              'Project Settings (JSON)',
              'Settings',
              'settings.openProject',
              'Open .lgtm/config.json in the editor',
            ),
          },
        },
        w: {
          key: 'w',
          label: '+window',
          group: 'Window / Pane',
          description: 'Spatial and indexed pane navigation',
          category: 'Window / Pane',
          proxy: ['Control+w'],
        },
      },
    },
  }

  /** Command handlers invoked by `WhichKeyController` when a chord or explorer item executes. */
  readonly commands: Record<string, KeymapHandler> = {
    'command.openConfigured': () => this.openConfiguredCommandSurface(),
    'command.openColon': () => this.openColonCommandMenu(),
    'workspace.toggleExplorer': () => this.toggleExplorer(),
    'workspace.openFolder': () => {
      void this.workspace.selectWorkspace('__open_folder__')
      return true
    },
    'projects.show': () => this.showSidebarMode('projects'),
    'projects.showExplorer': () => this.showSidebarMode('explorer'),
    'projects.add': () => {
      void this.workspace.selectWorkspace('__open_folder__')
      return true
    },
    'projects.newWorktree': () => this.openNewWorktree(),
    'projects.removeWorktree': () => this.removeActiveWorktree(),
    'file.save': () => {
      void this.workspace.flushSave()
      return true
    },
    'file.new': () => {
      void this.workspace.newFile()
      return true
    },
    'file.newFolder': () => {
      if (!this.sidebarOpen) this.sidebarOpen = true
      this.workspace.setSidebarWidget('explorer')
      this.workspace.startCreate('directory')
      return true
    },
    'settings.openGlobal': () => {
      void this.workspace.openPath('~/.lgtm/config.json', { title: 'Settings', focus: true })
      return true
    },
    'settings.openProject': () => {
      void this.workspace.openPath('.lgtm/config.json', {
        title: 'Settings (project)',
        focus: true,
      })
      return true
    },
  }

  /** Command catalog shared by `CommandMenu` and `CommandPalette`. */
  get commandItems(): CommandNode[] {
    return [
      {
        value: 'projects',
        command: 'projects',
        label: 'Toggle Projects Dock',
        description: 'Toggle the Projects and Worktrees dock pane open or closed',
        group: 'Projects',
        shortcut: ['Mod+Shift+B'],
        onSelect: () => {
          this.showSidebarMode('projects')
        },
      },
      {
        value: 'explorer',
        command: 'explorer',
        label: 'Toggle Explorer Dock',
        description: 'Toggle the active workspace widget dock pane open or closed',
        group: 'Projects',
        shortcut: ['Mod+B'],
        onSelect: () => {
          this.showSidebarMode('explorer')
        },
      },
      {
        value: 'project-add',
        command: 'project-add',
        label: 'Add Project',
        description: 'Open a folder and register its project in the active session',
        group: 'Projects',
        shortcut: ['Space', 'p', 'a'],
        onSelect: () => {
          void this.workspace.selectWorkspace('__open_folder__')
        },
      },
      {
        value: 'worktree-new',
        command: 'worktree-new',
        label: 'New Worktree',
        description: 'Create a managed Git worktree for the active project',
        group: 'Projects',
        shortcut: ['Space', 'p', 'n'],
        disabled: !this.workspace.activeProject,
        onSelect: () => {
          this.openNewWorktree()
        },
      },
      {
        value: 'worktree-remove',
        command: 'worktree-remove',
        label: 'Remove Active Worktree',
        description: 'Remove the currently active linked Git worktree',
        group: 'Projects',
        shortcut: ['Space', 'p', 'd'],
        disabled: !this.workspace.activeProject?.worktrees.some(
          (wt) => wt.path === this.workspace.root,
        ),
        onSelect: () => {
          this.removeActiveWorktree()
        },
      },
      {
        value: 'file-save',
        command: 'write',
        aliases: ['w'],
        label: 'Save File',
        description: 'Flush pending changes and save the active file',
        group: 'File',
        shortcut: ['Mod+S'],
        onSelect: () => {
          void this.workspace.flushSave()
        },
      },
      {
        value: 'settings-global',
        command: 'settings',
        label: 'Open Global Settings (JSON)',
        description: 'Open ~/.lgtm/config.json in the editor',
        group: 'Preferences',
        shortcut: ['Mod+,'],
        onSelect: () => {
          void this.workspace.openPath('~/.lgtm/config.json', { title: 'Settings', focus: true })
        },
      },
      {
        value: 'settings-project',
        command: 'settings-project',
        label: 'Open Project Settings (JSON)',
        description: 'Open .lgtm/config.json in the editor',
        group: 'Preferences',
        shortcut: ['Mod+Shift+,'],
        onSelect: () => {
          void this.workspace.openPath('.lgtm/config.json', {
            title: 'Settings (project)',
            focus: true,
          })
        },
      },
    ]
  }

  // Public --------------------------------------------------------------------

  /** Updates the open state of the command surface and clears any transient `:` override on close. */
  setCommandOpen(open: boolean) {
    this.commandOpen = open
    if (!open) {
      this.commandStyleOverride = null
    }
  }

  /** Opens the configured command surface (`Mod+k` or `Space :`). */
  openConfiguredCommandSurface(): boolean {
    if (this.commandOpen && this.commandStyleOverride === null) {
      this.setCommandOpen(false)
      return true
    }
    this.commandStyleOverride = null
    this.commandOpen = true
    return true
  }

  /** Opens the bottom-docked colon `CommandMenu` (`:`). */
  openColonCommandMenu(): boolean {
    this.commandStyleOverride = 'colon'
    this.commandOpen = true
    return true
  }

  /** Toggles the Explorer widget pane and moves focus to it when opened. */
  toggleExplorer(): boolean {
    this.workspace.toggleExplorerPane()
    if (this.workspace.explorerOpen) {
      void tick().then(() => {
        getDocumentWhichKeyController()?.focusPane('workspace')
      })
    }
    return true
  }

  /** Toggles the `explorer` or `projects` dock pane independently and focuses it when opened. */
  showSidebarMode(mode: 'explorer' | 'projects'): boolean {
    if (mode === 'projects') {
      this.workspace.toggleProjectsPane()
      if (this.workspace.projectsOpen) {
        void tick().then(() => {
          getDocumentWhichKeyController()?.focusPane('projects')
        })
      }
    } else {
      this.workspace.toggleExplorerPane()
      if (this.workspace.explorerOpen) {
        void tick().then(() => {
          getDocumentWhichKeyController()?.focusPane('workspace')
        })
      }
    }
    return true
  }

  /** Opens the `New Worktree` dialog in `ProjectsPane`. */
  openNewWorktree(): boolean {
    if (!this.sidebarOpen) this.sidebarOpen = true
    this.workspace.setSidebarWidget('projects')
    this.workspace.openNewWorktreeDialog()
    return true
  }

  /** Prompts to remove the currently active linked Git worktree in `ProjectsPane`. */
  removeActiveWorktree(): boolean {
    const project = this.workspace.activeProject
    const wt = project?.worktrees.find((item) => item.path === this.workspace.root)
    if (!project || !wt) return false
    if (!this.sidebarOpen) this.sidebarOpen = true
    this.workspace.setSidebarWidget('projects')
    this.workspace.promptRemoveWorktree(project.id, wt.path)
    return true
  }
}
