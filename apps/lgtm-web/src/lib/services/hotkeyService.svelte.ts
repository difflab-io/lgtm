import { tick } from 'svelte'
import {
  getDocumentWhichKeyController,
  type CommandNode,
  type CommandStyle as HelicalCommandStyle,
  type KeybindingPreset,
  type KeymapCommandContext,
  type KeymapHandler,
  type KeymapMode,
  type WhichKeyNode,
} from '@cloudvoyant/helical-ui'
import {
  gotoFirstEditorDiagnostic,
  gotoLastEditorDiagnostic,
  gotoNextEditorDiagnostic,
  gotoPrevEditorDiagnostic,
  scrollEditorAlign,
  scrollEditorHalfPage,
  showEditorHoverTooltip,
  showEditorSignatureHelpTooltip,
  triggerEditorCompletion,
  type EditorView,
} from 'lgtm-ui'
import type {
  CommandStyle,
  ConfigScope,
  KeybindPreset,
  LgtmConfigPatch,
  LineWrap,
  ThemeMode,
} from '../clients/workspaceClient'
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

interface BooleanSettingCommandOptions {
  value: string
  aliases?: string[]
  label: string
  actionNoun?: string
  description: string
  shortcut?: CommandNode['shortcut']
  current: boolean
  apply: (next: boolean) => void
}

function booleanSettingCommand({
  value,
  aliases,
  label,
  actionNoun = label,
  description,
  shortcut,
  current,
  apply,
}: BooleanSettingCommandOptions): CommandNode {
  return {
    value,
    command: value,
    ...(aliases ? { aliases } : {}),
    label,
    description,
    group: 'Preferences',
    shortcut,
    children: [
      {
        value: `${value}-on`,
        command: 'on',
        aliases: ['true', 'enable'],
        label: `Enable ${actionNoun}`,
        active: current,
        onSelect: () => {
          apply(true)
        },
      },
      {
        value: `${value}-off`,
        command: 'off',
        aliases: ['false', 'disable'],
        label: `Disable ${actionNoun}`,
        active: !current,
        onSelect: () => {
          apply(false)
        },
      },
      {
        value: `${value}-toggle`,
        command: 'toggle',
        label: `Toggle ${actionNoun}`,
        onSelect: () => {
          apply(!current)
        },
      },
    ],
  }
}

/**
 * Application-level modal keybinding, `WhichKey` leader menu, and `CommandMenu`/`CommandPalette` registry.
 */
export class HotkeyService {
  private readonly workspace: WorkspaceService

  /** Whether the workspace explorer sidebar is currently expanded. */
  sidebarOpen = $state(true)
  /** Whether the active command surface (`CommandMenu` or `CommandPalette`) is open. */
  commandOpen = $state(false)
  /** Transient override forcing `CommandMenu` (`colon`) when opened via `:`. */
  commandStyleOverride = $state<'colon' | null>(null)
  /** Currently mounted CodeMirror 6 `EditorView`, if a file or diagnostics buffer is open. */
  activeEditorView = $state<EditorView | null>(null)

  /** Constructs a `HotkeyService` bound to `workspace`. */
  constructor(workspace: WorkspaceService) {
    this.workspace = workspace
  }

  /** Effective Helical command surface style (`menu`, `palette`, or `colon`). */
  get effectiveCommandStyle(): HelicalCommandStyle {
    return (
      this.commandStyleOverride ??
      (this.workspace.settings.commandStyle === 'palette' ? 'palette' : 'menu')
    )
  }

  /** Application keymap merged above the active Helix, Vim, or Kakoune preset. */
  readonly keymap: Record<string, WhichKeyNode> = {
    'Mod+s': leaf(
      'Mod+s',
      'Save File',
      'File',
      'file.save',
      'Flush pending changes and save the active file',
      ['normal', 'select', 'insert'],
    ),
    'Mod+Shift+s': leaf(
      'Mod+Shift+s',
      'Save All',
      'File',
      'file.saveAll',
      'Flush all pending file saves',
      ['normal', 'select', 'insert'],
    ),
    'Mod+k': leaf(
      'Mod+k',
      'Command Surface',
      'Command',
      'command.openConfigured',
      'Open the configured CommandMenu or CommandPalette',
      ['normal', 'select', 'insert'],
    ),
    'Mod+b': leaf(
      'Mod+b',
      'Toggle Explorer',
      'Workspace',
      'workspace.toggleExplorer',
      'Toggle the workspace explorer sidebar open or closed',
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
    'Control+d': leaf(
      'Control+d',
      'Scroll Half Page Down',
      'View',
      'view.halfPageDown',
      'Scroll the active editor or pane down by half a page',
      ['normal', 'select'],
    ),
    'Control+u': leaf(
      'Control+u',
      'Scroll Half Page Up',
      'View',
      'view.halfPageUp',
      'Scroll the active editor or pane up by half a page',
      ['normal', 'select'],
    ),
    F12: leaf(
      'F12',
      'Go to Definition',
      'LSP',
      'lsp.gotoDefinition',
      'Jump to symbol definition at cursor (F12)',
      ['normal', 'select', 'insert'],
    ),
    K: leaf(
      'K',
      'Hover Documentation',
      'LSP',
      'lsp.hover',
      'Show LSP hover documentation at cursor (K)',
      ['normal', 'select'],
    ),
    'Control+Space': leaf(
      'Control+Space',
      'Trigger Autocomplete',
      'LSP',
      'lsp.triggerCompletion',
      'Trigger autocomplete suggestions at cursor (Ctrl+Space)',
      ['normal', 'select', 'insert'],
    ),
    'Control+x': leaf(
      'Control+x',
      'Trigger Autocomplete',
      'LSP',
      'lsp.triggerCompletion',
      'Trigger autocomplete suggestions at cursor (Ctrl+x)',
      ['insert'],
    ),
    'Control+Shift+Space': leaf(
      'Control+Shift+Space',
      'Signature Help',
      'LSP',
      'lsp.signatureHelp',
      'Show LSP function signature help at cursor (Ctrl+Shift+Space)',
      ['normal', 'select', 'insert'],
    ),
    'Alt+s': leaf(
      'Alt+s',
      'Signature Help',
      'LSP',
      'lsp.signatureHelp',
      'Show LSP function signature help at cursor (Alt+s)',
      ['normal', 'select', 'insert'],
    ),
    '[': {
      key: '[',
      label: 'Previous',
      group: 'Previous (Unimpaired)',
      category: 'Unimpaired',
      mode: ['normal', 'select'],
      children: {
        d: leaf(
          'd',
          'Previous Diagnostic',
          'LSP',
          'lsp.prevDiagnostic',
          'Jump to previous diagnostic in file ([d)',
        ),
        D: leaf(
          'D',
          'First Diagnostic',
          'LSP',
          'lsp.firstDiagnostic',
          'Jump to first diagnostic in file ([D)',
        ),
      },
    },
    ']': {
      key: ']',
      label: 'Next',
      group: 'Next (Unimpaired)',
      category: 'Unimpaired',
      mode: ['normal', 'select'],
      children: {
        d: leaf(
          'd',
          'Next Diagnostic',
          'LSP',
          'lsp.nextDiagnostic',
          'Jump to next diagnostic in file (]d)',
        ),
        D: leaf(
          'D',
          'Last Diagnostic',
          'LSP',
          'lsp.lastDiagnostic',
          'Jump to last diagnostic in file (]D)',
        ),
      },
    },
    g: {
      key: 'g',
      label: 'Go to',
      group: 'Go to',
      category: 'Goto',
      mode: ['normal', 'select'],
      children: {
        d: leaf(
          'd',
          'Go to Definition',
          'LSP',
          'lsp.gotoDefinition',
          'Jump to symbol definition at cursor (gd)',
        ),
        K: leaf(
          'K',
          'Signature Help',
          'LSP',
          'lsp.signatureHelp',
          'Show LSP function signature help at cursor (gK)',
        ),
      },
    },
    Space: {
      key: 'Space',
      label: '+leader',
      group: 'Leader',
      category: 'Leader',
      mode: ['normal', 'select'],
      expand: () => {
        const viewKey = this.controller?.getState().preset === 'kakoune' ? 'v' : 'z'
        return {
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
            'Open the configured CommandMenu or CommandPalette',
          ),
          e: leaf(
            'e',
            'Toggle Explorer',
            'Workspace',
            'workspace.toggleExplorer',
            'Toggle the workspace explorer sidebar',
          ),
          f: {
            key: 'f',
            label: '+file',
            group: 'File',
            description: 'File and workspace operations',
            category: 'File',
            children: {
              s: leaf('s', 'Save File', 'File', 'file.save', 'Save the active file'),
              S: leaf('S', 'Save All', 'File', 'file.saveAll', 'Save all modified files'),
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
          b: {
            key: 'b',
            label: '+buffer',
            group: 'Buffer / Tabs',
            description: 'Navigate and close open editor tabs',
            category: 'Buffer',
            children: {
              n: leaf('n', 'Next Tab', 'Buffer', 'tab.next', 'Switch to next open tab'),
              p: leaf('p', 'Previous Tab', 'Buffer', 'tab.prev', 'Switch to previous open tab'),
              d: leaf('d', 'Close Tab', 'Buffer', 'tab.close', 'Close the active editor tab'),
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
          l: {
            key: 'l',
            label: '+lsp',
            group: 'LSP',
            description: 'Language server navigation, IntelliSense, and diagnostics',
            category: 'LSP',
            children: {
              g: leaf(
                'g',
                'Go to Definition',
                'LSP',
                'lsp.gotoDefinition',
                'Jump to symbol definition at cursor',
              ),
              k: leaf(
                'k',
                'Hover Documentation',
                'LSP',
                'lsp.hover',
                'Show LSP hover documentation at cursor',
              ),
              h: leaf(
                'h',
                'Hover Documentation',
                'LSP',
                'lsp.hover',
                'Show LSP hover documentation at cursor',
              ),
              s: leaf(
                's',
                'Signature Help',
                'LSP',
                'lsp.signatureHelp',
                'Show LSP function signature help at cursor',
              ),
              c: leaf(
                'c',
                'Trigger Autocomplete',
                'LSP',
                'lsp.triggerCompletion',
                'Trigger autocomplete suggestions at cursor',
              ),
              n: leaf(
                'n',
                'Next Diagnostic',
                'LSP',
                'lsp.nextDiagnostic',
                'Jump to next diagnostic in file',
              ),
              p: leaf(
                'p',
                'Previous Diagnostic',
                'LSP',
                'lsp.prevDiagnostic',
                'Jump to previous diagnostic in file',
              ),
              d: leaf(
                'd',
                'Open Diagnostics',
                'LSP',
                'lsp.diagnostics',
                'Open workspace diagnostics buffer',
              ),
              r: leaf(
                'r',
                'Restart Language Server',
                'LSP',
                'lsp.restart',
                'Restart active language servers',
              ),
            },
          },
          u: {
            key: 'u',
            label: '+preferences',
            group: 'Preferences',
            description: 'Update user settings in config.json',
            category: 'Preferences',
            children: {
              t: {
                key: 't',
                label: '+theme',
                group: 'Theme',
                category: 'Preferences',
                children: {
                  d: leaf(
                    'd',
                    'Dark Theme',
                    'Preferences',
                    'settings.theme.dark',
                    'Set appearance.theme to dark',
                  ),
                  l: leaf(
                    'l',
                    'Light Theme',
                    'Preferences',
                    'settings.theme.light',
                    'Set appearance.theme to light',
                  ),
                  s: leaf(
                    's',
                    'System Theme',
                    'Preferences',
                    'settings.theme.system',
                    'Set appearance.theme to system',
                  ),
                },
              },
              c: {
                key: 'c',
                label: '+command style',
                group: 'Command Style',
                category: 'Preferences',
                children: {
                  m: leaf(
                    'm',
                    'Command Menu',
                    'Preferences',
                    'settings.commandStyle.menu',
                    'Use bottom-docked CommandMenu for Mod+k',
                  ),
                  p: leaf(
                    'p',
                    'Command Palette',
                    'Preferences',
                    'settings.commandStyle.palette',
                    'Use modal CommandPalette for Mod+k',
                  ),
                },
              },
              k: {
                key: 'k',
                label: '+preset',
                group: 'Keybinding Preset',
                category: 'Preferences',
                children: {
                  h: leaf(
                    'h',
                    'Helix Preset',
                    'Preferences',
                    'preset.helix',
                    'Switch to Helix modal grammar',
                  ),
                  v: leaf(
                    'v',
                    'Vim Preset',
                    'Preferences',
                    'preset.vim',
                    'Switch to Vim modal grammar',
                  ),
                  k: leaf(
                    'k',
                    'Kakoune Preset',
                    'Preferences',
                    'preset.kakoune',
                    'Switch to Kakoune modal grammar',
                  ),
                },
              },
              m: leaf(
                'm',
                'Toggle Leader-Only Mode',
                'Preferences',
                'settings.toggleLeaderOnly',
                'Toggle keybindings.leaderOnly in config.json',
              ),
              o: leaf(
                'o',
                'Toggle Pane Outline',
                'Preferences',
                'settings.togglePaneOutline',
                'Toggle keybindings.paneOutline in config.json',
              ),
              i: leaf(
                'i',
                'Toggle Click to Insert',
                'Preferences',
                'settings.toggleClickToInsert',
                'Toggle keybindings.clickToInsert in config.json',
              ),
              w: {
                key: 'w',
                label: '+line wrap',
                group: 'Line Wrapping',
                category: 'Preferences',
                children: {
                  a: leaf(
                    'a',
                    'Wrap Auto',
                    'Preferences',
                    'settings.wrap.auto',
                    'Wrap lines at viewport width',
                  ),
                  o: leaf(
                    'o',
                    'Wrap Off',
                    'Preferences',
                    'settings.wrap.off',
                    'Disable line wrapping',
                  ),
                  '8': leaf(
                    '8',
                    'Wrap 80 Columns',
                    'Preferences',
                    'settings.wrap.80',
                    'Wrap lines at 80 columns',
                  ),
                  '1': leaf(
                    '1',
                    'Wrap 100 Columns',
                    'Preferences',
                    'settings.wrap.100',
                    'Wrap lines at 100 columns',
                  ),
                  '2': leaf(
                    '2',
                    'Wrap 120 Columns',
                    'Preferences',
                    'settings.wrap.120',
                    'Wrap lines at 120 columns',
                  ),
                },
              },
              l: leaf(
                'l',
                'Toggle LSP Enabled',
                'Preferences',
                'settings.toggleLsp',
                'Toggle lsp.enabled in config.json',
              ),
            },
          },
          g: {
            key: 'g',
            label: '+goto',
            group: 'Goto',
            description: 'Go to locations and boundaries',
            category: 'Goto',
            proxy: ['g'],
          },
          v: {
            key: 'v',
            label: '+view',
            group: 'View / Scroll',
            description: 'Scroll and align viewport',
            category: 'View',
            proxy: [viewKey],
          },
          j: leaf(
            'j',
            'Jump to Element',
            'Navigation',
            'jump.start',
            'Show 2D home-row jump labels over visible controls',
          ),
          k: leaf(
            'k',
            'Hover Documentation',
            'LSP',
            'lsp.hover',
            'Show LSP hover documentation at cursor (Space k)',
          ),
          s: leaf(
            's',
            'Signature Help',
            'LSP',
            'lsp.signatureHelp',
            'Show LSP function signature help at cursor (Space s)',
          ),
          d: leaf(
            'd',
            'Open Diagnostics',
            'LSP',
            'lsp.diagnostics',
            'Open workspace diagnostics buffer (Space d)',
          ),
          D: leaf(
            'D',
            'Open Workspace Diagnostics',
            'LSP',
            'lsp.diagnostics',
            'Open workspace diagnostics buffer (Space D)',
          ),
        }
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
    'file.save': () => {
      void this.workspace.flushSave()
      return true
    },
    'file.saveAll': () => {
      void this.workspace.flushSave()
      return true
    },
    'file.new': () => {
      void this.workspace.newFile()
      return true
    },
    'file.newFolder': () => {
      if (!this.sidebarOpen) this.sidebarOpen = true
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
    'tab.next': (ctx: KeymapCommandContext) => {
      if (this.workspace.tabs.length <= 1) return false
      void this.workspace.selectNextTab(ctx.count)
      return true
    },
    'tab.prev': (ctx: KeymapCommandContext) => {
      if (this.workspace.tabs.length <= 1) return false
      void this.workspace.selectPrevTab(ctx.count)
      return true
    },
    'tab.close': () => {
      void this.workspace.closeActiveTab()
      return true
    },
    'lsp.gotoDefinition': () => this.gotoDefinitionAtCursor(),
    'lsp.hover': () => this.hoverAtCursor(),
    'lsp.signatureHelp': () => this.signatureHelpAtCursor(),
    'lsp.triggerCompletion': () => this.triggerCompletionAtCursor(),
    'lsp.nextDiagnostic': (ctx: KeymapCommandContext) => this.nextDiagnostic(ctx.count),
    'lsp.prevDiagnostic': (ctx: KeymapCommandContext) => this.prevDiagnostic(ctx.count),
    'lsp.firstDiagnostic': () => this.firstDiagnostic(),
    'lsp.lastDiagnostic': () => this.lastDiagnostic(),
    'lsp.diagnostics': () => {
      void this.workspace.openDiagnosticsBuffer()
      return true
    },
    'lsp.restart': () => this.restartLanguageServers(),
    'view.center': () => {
      if (this.activeEditorView && this.isEditorActive()) {
        return scrollEditorAlign(this.activeEditorView, 'center')
      }
      return false
    },
    'view.top': () => {
      if (this.activeEditorView && this.isEditorActive()) {
        return scrollEditorAlign(this.activeEditorView, 'top')
      }
      return false
    },
    'view.bottom': () => {
      if (this.activeEditorView && this.isEditorActive()) {
        return scrollEditorAlign(this.activeEditorView, 'bottom')
      }
      return false
    },
    'view.scrollDown': (ctx: KeymapCommandContext) => {
      if (this.activeEditorView && this.isEditorActive()) {
        return scrollEditorHalfPage(this.activeEditorView, 1, ctx.count)
      }
      return false
    },
    'view.scrollUp': (ctx: KeymapCommandContext) => {
      if (this.activeEditorView && this.isEditorActive()) {
        return scrollEditorHalfPage(this.activeEditorView, -1, ctx.count)
      }
      return false
    },
    'view.halfPageDown': (ctx: KeymapCommandContext) => {
      if (this.activeEditorView && this.isEditorActive()) {
        return scrollEditorHalfPage(this.activeEditorView, 1, ctx.count)
      }
      return false
    },
    'view.halfPageUp': (ctx: KeymapCommandContext) => {
      if (this.activeEditorView && this.isEditorActive()) {
        return scrollEditorHalfPage(this.activeEditorView, -1, ctx.count)
      }
      return false
    },
    'settings.theme.dark': () => this.setTheme('dark'),
    'settings.theme.light': () => this.setTheme('light'),
    'settings.theme.system': () => this.setTheme('system'),
    'settings.commandStyle.menu': () => this.setCommandStyle('menu'),
    'settings.commandStyle.palette': () => this.setCommandStyle('palette'),
    'settings.toggleLeaderOnly': () => this.setLeaderOnly(!this.workspace.settings.leaderOnly),
    'settings.togglePaneOutline': () => this.setPaneOutline(!this.workspace.settings.paneOutline),
    'settings.toggleClickToInsert': () =>
      this.setClickToInsert(!this.workspace.settings.clickToInsert),
    'settings.wrap.auto': () => this.setLineWrapping('auto'),
    'settings.wrap.off': () => this.setLineWrapping('off'),
    'settings.wrap.80': () => this.setLineWrapping(80),
    'settings.wrap.100': () => this.setLineWrapping(100),
    'settings.wrap.120': () => this.setLineWrapping(120),
    'settings.toggleLsp': () => this.setLspEnabled(!this.workspace.settings.lspEnabled),
  }

  /** Hierarchical command catalog shared by `CommandMenu` and `CommandPalette`. */
  get commandItems(): CommandNode[] {
    return [
      {
        value: 'write',
        command: 'write',
        aliases: ['w'],
        keywords: ['save'],
        label: 'Save File',
        description: 'Flush pending changes and save the active file',
        group: 'File',
        shortcut: 'Mod+S',
        onSelect: () => {
          void this.workspace.flushSave()
        },
      },
      {
        value: 'write-all',
        command: 'write-all',
        aliases: ['wa'],
        keywords: ['save-all'],
        label: 'Save All',
        description: 'Flush all pending file saves',
        group: 'File',
        shortcut: 'Mod+Shift+S',
        onSelect: () => {
          void this.workspace.flushSave()
        },
      },
      {
        value: 'quit-tab',
        command: 'quit',
        aliases: ['q', 'bd'],
        keywords: ['close', 'buffer-close'],
        label: 'Close Active Tab',
        description: 'Close the active editor tab',
        group: 'Buffer',
        shortcut: 'Space b d',
        onSelect: () => {
          void this.workspace.closeActiveTab()
        },
      },
      {
        value: 'buffer-next',
        command: 'buffer-next',
        aliases: ['bn'],
        keywords: ['bnext', 'tab-next'],
        label: 'Next Tab',
        description: 'Cycle to the next open editor tab',
        group: 'Buffer',
        shortcut: [']', 't'],
        onSelect: () => {
          void this.workspace.selectNextTab(1)
        },
      },
      {
        value: 'buffer-prev',
        command: 'buffer-prev',
        aliases: ['bp'],
        keywords: ['bprev', 'tab-prev'],
        label: 'Previous Tab',
        description: 'Cycle to the previous open editor tab',
        group: 'Buffer',
        shortcut: ['[', 't'],
        onSelect: () => {
          void this.workspace.selectPrevTab(1)
        },
      },
      {
        value: 'new-file',
        command: 'new',
        keywords: ['new-file'],
        label: 'New File',
        description: 'Create a new file in the workspace',
        group: 'File',
        shortcut: 'Space f n',
        onSelect: () => {
          void this.workspace.newFile()
        },
      },
      {
        value: 'new-folder',
        command: 'mkdir',
        keywords: ['new-folder'],
        label: 'New Folder',
        description: 'Create a new folder in the workspace',
        group: 'File',
        shortcut: 'Space f N',
        onSelect: () => {
          if (!this.sidebarOpen) this.sidebarOpen = true
          this.workspace.startCreate('directory')
        },
      },
      {
        value: 'toggle-sidebar',
        command: 'sidebar',
        keywords: ['explorer', 'tree'],
        label: 'Toggle Workspace Sidebar',
        description: 'Show or hide the workspace explorer sidebar',
        group: 'Workspace',
        shortcut: 'Mod+B',
        onSelect: () => {
          this.toggleExplorer()
        },
      },
      {
        value: 'open-config',
        command: 'config',
        keywords: ['settings', 'preferences'],
        label: 'Open Global Settings (config.json)',
        description: 'Open ~/.lgtm/config.json in an editor tab',
        group: 'Preferences',
        shortcut: 'Space f c',
        onSelect: () => {
          void this.workspace.openPath('~/.lgtm/config.json', { title: 'Settings', focus: true })
        },
      },
      {
        value: 'open-project-config',
        command: 'config-project',
        keywords: ['settings-project'],
        label: 'Open Project Settings (.lgtm/config.json)',
        description: 'Open .lgtm/config.json in an editor tab',
        group: 'Preferences',
        shortcut: 'Space f p',
        onSelect: () => {
          void this.workspace.openPath('.lgtm/config.json', {
            title: 'Settings (project)',
            focus: true,
          })
        },
      },
      {
        value: 'goto-definition',
        command: 'goto-definition',
        aliases: ['gd'],
        keywords: ['definition'],
        label: 'Go to Definition',
        description: 'Jump to the LSP definition at the current cursor position',
        group: 'LSP',
        shortcut: 'F12',
        onSelect: () => {
          this.gotoDefinitionAtCursor()
        },
      },
      {
        value: 'lsp-hover',
        command: 'hover',
        aliases: ['K'],
        keywords: ['doc'],
        label: 'Hover Documentation',
        description: 'Show LSP hover documentation at the current cursor position',
        group: 'LSP',
        shortcut: 'K',
        onSelect: () => {
          this.hoverAtCursor()
        },
      },
      {
        value: 'lsp-signature-help',
        command: 'signature',
        aliases: ['sig'],
        keywords: ['signature-help'],
        label: 'Signature Help',
        description: 'Show LSP function signature help at the current cursor position',
        group: 'LSP',
        shortcut: 'Space l s',
        onSelect: () => {
          this.signatureHelpAtCursor()
        },
      },
      {
        value: 'lsp-complete',
        command: 'complete',
        keywords: ['completion', 'intellisense'],
        label: 'Trigger Autocomplete',
        description: 'Open autocomplete suggestions at the current cursor position',
        group: 'LSP',
        shortcut: 'Control+Space',
        onSelect: () => {
          this.triggerCompletionAtCursor()
        },
      },
      {
        value: 'diagnostic-next',
        command: 'diagnostic-next',
        aliases: ['dn'],
        keywords: ['lnext'],
        label: 'Next Diagnostic',
        description: 'Jump to the next inline diagnostic in the active file',
        group: 'LSP',
        shortcut: [']', 'd'],
        onSelect: () => {
          this.nextDiagnostic(1)
        },
      },
      {
        value: 'diagnostic-prev',
        command: 'diagnostic-prev',
        aliases: ['dp'],
        keywords: ['lprev'],
        label: 'Previous Diagnostic',
        description: 'Jump to the previous inline diagnostic in the active file',
        group: 'LSP',
        shortcut: ['[', 'd'],
        onSelect: () => {
          this.prevDiagnostic(1)
        },
      },
      {
        value: 'diagnostics',
        command: 'diagnostics',
        aliases: ['diag'],
        keywords: ['problems'],
        label: 'Open Workspace Diagnostics',
        description: 'Open the workspace LSP diagnostics buffer',
        group: 'LSP',
        shortcut: 'Space l d',
        onSelect: () => {
          void this.workspace.openDiagnosticsBuffer()
        },
      },
      {
        value: 'lsp-restart',
        command: 'lsp-restart',
        keywords: ['restart-lsp'],
        label: 'Restart Language Server',
        description: 'Restart active language servers for this workspace',
        group: 'LSP',
        shortcut: 'Space l r',
        onSelect: () => {
          this.restartLanguageServers()
        },
      },
      {
        value: 'theme',
        command: 'theme',
        aliases: ['t'],
        keywords: ['colorscheme'],
        label: 'Theme',
        description: 'Switch color theme (dark, light, or system)',
        group: 'Preferences',
        shortcut: 'Space u t',
        children: [
          {
            value: 'theme-dark',
            command: 'dark',
            label: 'Dark',
            description: 'Use dark color theme',
            active: this.workspace.settings.theme === 'dark',
            onSelect: () => {
              this.setTheme('dark')
            },
          },
          {
            value: 'theme-light',
            command: 'light',
            label: 'Light',
            description: 'Use light color theme',
            active: this.workspace.settings.theme === 'light',
            onSelect: () => {
              this.setTheme('light')
            },
          },
          {
            value: 'theme-system',
            command: 'system',
            aliases: ['auto'],
            label: 'System',
            description: 'Follow OS appearance setting',
            active: this.workspace.settings.theme === 'system',
            onSelect: () => {
              this.setTheme('system')
            },
          },
        ],
      },
      {
        value: 'command-style',
        command: 'command-style',
        keywords: ['cmdstyle'],
        label: 'Command Surface Style',
        description: 'Choose whether Mod+k opens CommandMenu or CommandPalette',
        group: 'Preferences',
        shortcut: 'Space u c',
        children: [
          {
            value: 'command-style-menu',
            command: 'menu',
            label: 'Command Menu (Bottom Bar)',
            description: 'Open bottom-docked CommandMenu by default',
            active: this.workspace.settings.commandStyle === 'menu',
            onSelect: () => {
              this.setCommandStyle('menu')
            },
          },
          {
            value: 'command-style-palette',
            command: 'palette',
            label: 'Command Palette (Modal)',
            description: 'Open centered CommandPalette modal by default',
            active: this.workspace.settings.commandStyle === 'palette',
            onSelect: () => {
              this.setCommandStyle('palette')
            },
          },
        ],
      },
      {
        value: 'preset',
        command: 'preset',
        aliases: ['k'],
        keywords: ['keymap', 'keybinds'],
        label: 'Keybinding Preset',
        description: 'Switch modal editing grammar (helix, vim, or kakoune)',
        group: 'Preferences',
        shortcut: 'Space u k',
        children: [
          {
            value: 'preset-helix',
            command: 'helix',
            label: 'Helix',
            description: 'Selection-first Helix modal grammar (default)',
            active: this.workspace.settings.keybindPreset === 'helix',
            onSelect: () => {
              this.setPreset('helix')
            },
          },
          {
            value: 'preset-vim',
            command: 'vim',
            label: 'Vim',
            description: 'Operator-motion Vim modal grammar',
            active: this.workspace.settings.keybindPreset === 'vim',
            onSelect: () => {
              this.setPreset('vim')
            },
          },
          {
            value: 'preset-kakoune',
            command: 'kakoune',
            aliases: ['kak'],
            label: 'Kakoune',
            description: 'Selection-first Kakoune modal grammar',
            active: this.workspace.settings.keybindPreset === 'kakoune',
            onSelect: () => {
              this.setPreset('kakoune')
            },
          },
        ],
      },
      booleanSettingCommand({
        value: 'leader-only',
        label: 'Leader-Only Mode',
        description: 'Only open WhichKey popup for Space leader sequences',
        shortcut: 'Space u m',
        current: this.workspace.settings.leaderOnly,
        apply: (next) => {
          this.setLeaderOnly(next)
        },
      }),
      booleanSettingCommand({
        value: 'pane-outline',
        label: 'Active Pane Outline',
        description: 'Show or hide focus outline around the active spatial pane',
        shortcut: 'Space u o',
        current: this.workspace.settings.paneOutline,
        apply: (next) => {
          this.setPaneOutline(next)
        },
      }),
      booleanSettingCommand({
        value: 'click-to-insert',
        label: 'Click Editor to Insert',
        actionNoun: 'Click to Insert',
        description: 'Enter insert mode automatically when clicking inside the code editor',
        shortcut: 'Space u i',
        current: this.workspace.settings.clickToInsert,
        apply: (next) => {
          this.setClickToInsert(next)
        },
      }),
      {
        value: 'wrap',
        command: 'wrap',
        keywords: ['line-wrap'],
        label: 'Editor Line Wrapping',
        description: 'Configure editor line wrapping (auto, off, or column width)',
        group: 'Preferences',
        shortcut: 'Space u w',
        children: [
          {
            value: 'wrap-auto',
            command: 'auto',
            aliases: ['on'],
            label: 'Auto (Viewport Width)',
            active: this.workspace.settings.lineWrapping === 'auto',
            onSelect: () => {
              this.setLineWrapping('auto')
            },
          },
          {
            value: 'wrap-off',
            command: 'off',
            label: 'Off (No Wrapping)',
            active: this.workspace.settings.lineWrapping === 'off',
            onSelect: () => {
              this.setLineWrapping('off')
            },
          },
          {
            value: 'wrap-80',
            command: '80',
            label: '80 Columns',
            active: this.workspace.settings.lineWrapping === 80,
            onSelect: () => {
              this.setLineWrapping(80)
            },
          },
          {
            value: 'wrap-100',
            command: '100',
            label: '100 Columns',
            active: this.workspace.settings.lineWrapping === 100,
            onSelect: () => {
              this.setLineWrapping(100)
            },
          },
          {
            value: 'wrap-120',
            command: '120',
            label: '120 Columns',
            active: this.workspace.settings.lineWrapping === 120,
            onSelect: () => {
              this.setLineWrapping(120)
            },
          },
        ],
      },
      booleanSettingCommand({
        value: 'lsp-mode',
        aliases: ['lsp'],
        label: 'LSP Integration',
        actionNoun: 'LSP',
        description: 'Enable or disable workspace language servers',
        shortcut: 'Space u l',
        current: this.workspace.settings.lspEnabled,
        apply: (next) => {
          this.setLspEnabled(next)
        },
      }),
      {
        value: 'keybindings-help',
        command: 'keybindings',
        aliases: ['keys'],
        keywords: ['help'],
        label: 'Explore Keybindings',
        description: 'Open the searchable WhichKeyExplorer modal',
        group: 'Help',
        shortcut: '?',
        onSelect: () => {
          this.controller?.openExplorer()
        },
      },
    ]
  }

  /** Registers or clears the active CodeMirror 6 `EditorView` instance. */
  setEditorView(view: EditorView | null): void {
    this.activeEditorView = view
  }

  /** Updates the open state of the active command surface and resets any transient style override on close. */
  setCommandOpen(open: boolean): void {
    this.commandOpen = open
    if (!open) {
      this.commandStyleOverride = null
    }
  }

  /** Opens the bottom-docked colon `CommandMenu` regardless of the configured `commandStyle`. */
  openColonCommandMenu(): boolean {
    this.commandStyleOverride = 'colon'
    this.commandOpen = true
    return true
  }

  /** Opens the user's configured command surface (`CommandMenu` by default, or `CommandPalette`). */
  openConfiguredCommandSurface(): boolean {
    this.commandStyleOverride = null
    this.commandOpen = true
    return true
  }

  /** Toggles the workspace explorer sidebar and moves focus to the resulting active pane. */
  toggleExplorer(): boolean {
    const nextOpen = !this.sidebarOpen
    this.sidebarOpen = nextOpen
    const targetPane = nextOpen ? 'workspace' : 'editor'
    void tick().then(() => {
      requestAnimationFrame(() => {
        this.controller?.focusPane(targetPane)
      })
    })
    return true
  }

  /** Persists a modal preset change (`helix`, `vim`, or `kakoune`) to `config.json`. */
  setPreset(preset: KeybindingPreset | KeybindPreset, scope?: ConfigScope | null): boolean {
    this.controller?.setPreset(preset)
    return this.applySettingsPatch({ keybindings: { preset } }, scope)
  }

  /** Persists a `leaderOnly` change to `config.json`. */
  setLeaderOnly(leaderOnly: boolean, scope?: ConfigScope | null): boolean {
    this.controller?.setLeaderOnly(leaderOnly)
    return this.applySettingsPatch({ keybindings: { leaderOnly } }, scope)
  }

  /** Persists a `paneOutline` change to `config.json`. */
  setPaneOutline(paneOutline: boolean, scope?: ConfigScope | null): boolean {
    return this.applySettingsPatch({ keybindings: { paneOutline } }, scope)
  }

  /** Persists a `clickToInsert` change to `config.json`. */
  setClickToInsert(clickToInsert: boolean, scope?: ConfigScope | null): boolean {
    return this.applySettingsPatch({ keybindings: { clickToInsert } }, scope)
  }

  /** Persists a `commandStyle` change (`menu` or `palette`) to `config.json`. */
  setCommandStyle(commandStyle: CommandStyle, scope?: ConfigScope | null): boolean {
    this.commandStyleOverride = null
    return this.applySettingsPatch({ keybindings: { commandStyle } }, scope)
  }

  /** Persists a color `theme` change (`dark`, `light`, or `system`) to `config.json`. */
  setTheme(theme: ThemeMode, scope?: ConfigScope | null): boolean {
    return this.applySettingsPatch({ appearance: { theme } }, scope)
  }

  /** Persists an editor `lineWrapping` change (`auto`, `off`, or column count) to `config.json`. */
  setLineWrapping(lineWrapping: LineWrap, scope?: ConfigScope | null): boolean {
    return this.applySettingsPatch({ editor: { lineWrapping } }, scope)
  }

  /** Persists an `lsp.enabled` change to `config.json`. */
  setLspEnabled(enabled: boolean, scope?: ConfigScope | null): boolean {
    return this.applySettingsPatch({ lsp: { enabled } }, scope)
  }

  private get controller() {
    return typeof document !== 'undefined' ? getDocumentWhichKeyController(document) : null
  }

  private isEditorActive(): boolean {
    const activePane = this.controller?.getState().activePaneId
    if (activePane === 'editor') return true
    if (!this.activeEditorView || typeof document === 'undefined') return false
    return this.activeEditorView.dom.contains(document.activeElement)
  }

  private gotoDefinitionAtCursor(): boolean {
    if (!this.activeEditorView || !this.workspace.selected) return false
    const pos = this.activeEditorView.state.selection.main.head
    void this.workspace.gotoDefinition(pos)
    return true
  }

  private hoverAtCursor(): boolean {
    const view = this.activeEditorView
    if (!view || !this.workspace.selected) return false
    const pos = view.state.selection.main.head
    void this.workspace.requestHover(pos).then((hover) => {
      if (hover && this.activeEditorView === view) {
        showEditorHoverTooltip(view, hover)
      }
    })
    return true
  }

  private signatureHelpAtCursor(): boolean {
    const view = this.activeEditorView
    if (!view || !this.workspace.selected) return false
    const pos = view.state.selection.main.head
    void this.workspace.requestSignatureHelp(pos).then((help) => {
      if (help && this.activeEditorView === view) {
        showEditorSignatureHelpTooltip(view, help)
      }
    })
    return true
  }

  private triggerCompletionAtCursor(): boolean {
    const view = this.activeEditorView
    if (!view || view.state.readOnly) return false
    const run = () => {
      if (this.activeEditorView !== view) return
      if (this.controller && this.controller.getState().mode !== 'insert') {
        this.controller.setMode('insert')
      }
      view.focus()
      triggerEditorCompletion(view)
    }
    if (this.commandOpen) {
      void tick().then(() => {
        requestAnimationFrame(run)
      })
      return true
    }
    run()
    return true
  }

  private nextDiagnostic(count = 1): boolean {
    const view = this.activeEditorView
    if (!view) return false
    return gotoNextEditorDiagnostic(view, count)
  }

  private prevDiagnostic(count = 1): boolean {
    const view = this.activeEditorView
    if (!view) return false
    return gotoPrevEditorDiagnostic(view, count)
  }

  private firstDiagnostic(): boolean {
    const view = this.activeEditorView
    if (!view) return false
    return gotoFirstEditorDiagnostic(view)
  }

  private lastDiagnostic(): boolean {
    const view = this.activeEditorView
    if (!view) return false
    return gotoLastEditorDiagnostic(view)
  }

  private restartLanguageServers(): boolean {
    const activeLang = this.workspace.activeLanguageId
    if (activeLang) {
      void this.workspace.restartLanguageServer(activeLang)
      return true
    }
    const servers = Object.values(this.workspace.lsp.servers)
    if (servers.length === 0) return false
    for (const server of servers) {
      void this.workspace.restartLanguageServer(server.languageId)
    }
    return true
  }

  private applySettingsPatch(patch: LgtmConfigPatch, scope?: ConfigScope | null): boolean {
    void this.workspace.settings.update(patch, scope).then(async () => {
      await this.workspace.reloadCleanConfigTabs()
      if (patch.lsp !== undefined) {
        await this.workspace.syncLspState()
      }
    })
    return true
  }
}
