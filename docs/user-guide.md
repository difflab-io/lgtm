# LGTM User Guide

## Modal Editing and Keybindings

LGTM uses modal editing powered by Helical UI. The footer status bar displays the active preset and mode (`normal`, `select`, `insert`, or `jump`).

### Editing Presets

Switch between three modal presets in `config.json` or from the command menu:

| Preset | Philosophy | Line Selection | Undo / Redo |
| --- | --- | --- | --- |
| `helix` (default) | Selection first + `gd` goto and `Space` leader menus | `x` (extend line) | `u` / `U` |
| `vim` | Operator + motion (`dw`, `ciw`, `yy`) | `V` / `v` (`select`) | `u` / `U` |
| `kakoune` | Selection first, action second (`wd`, `wc`) | `x` (extend line) | `u` / `U` |

### Core Motions and Operators

| Key | Action |
| --- | --- |
| `h`, `j`, `k`, `l` | Move cursor left, down, up, right |
| `w`, `b`, `e` | Move by word start, previous word, word end |
| `0`, `^`, `$` / `gh`, `gs`, `gl` | Move to line start, first non-whitespace character, line end |
| `gg`, `ge` / `G` | Jump to document start or end |
| `f<char>`, `t<char>` | Jump to or before `<char>` on the current line |
| `mm` / `%` | Jump to matching bracket |
| `i`, `a`, `I`, `A`, `o`, `O` | Enter `insert` mode |
| `Escape` | Collapse selection and return to `normal` mode |
| `v` | Toggle `select` mode |
| `x` | Extend selection by full line |
| `m i <obj>`, `m a <obj>` | Select inner or around text object (`w`, `p`, `(`, `[`, `{`, `"`, `'`, `` ` ``) |
| `d`, `c`, `y`, `p`, `P` | Delete, change, yank, or paste selection |
| `C` / `Alt+j`, `Alt+k`, `,` | Add secondary cursor below/above or collapse to primary cursor |
| `Control+c` / `gc` | Toggle line comments |
| `Space j` | Activate two-character Flash jump labels across visible elements |

### Leader Shortcuts (`Space`)

Press `Space` in `normal` or `select` mode to open the WhichKey popup:

| Sequence | Action |
| --- | --- |
| `Space e` | Toggle workspace explorer sidebar |
| `Space f s` / `Space f S` | Save active file or save all files |
| `Space f n` / `Space f N` | Create new file or new folder |
| `Space f o` | Switch workspace folder |
| `Space f c` / `Space f p` | Open global settings (`~/.lgtm/config.json`) or project settings (`.lgtm/config.json`) |
| `Space b n` / `Space b p` / `Space b d` | Next tab, previous tab, close active tab |
| `Space w h` / `Space w l` / `Space w w` | Focus pane left/right or cycle panes |
| `Space w e` / `Space w c` | Focus explorer pane or editor pane directly |
| `Space l g` / `Space l d` / `Space l r` | Go to definition, open diagnostics, or restart language servers |
| `Space u t` / `Space u c` / `Space u k` | Switch theme, command surface style, or keybinding preset |
| `Space u m` / `Space u o` / `Space u i` | Toggle leader-only mode, active pane outline, or click-to-insert |
| `Space u w` / `Space u l` | Set line wrapping (`auto`, `off`, `80`, `100`, `120`) or toggle LSP |
| `Space g` | Go to start/end, definition, tab, or jumplist location |
| `Space z` / `Space v` | Toggle `select` mode |
| `Space j` / `Space k` | Start two-character jump mode or switch jump action (`activate` / `focus`) |
| `Space ?` / `Space :` | Open WhichKey hotkey explorer or colon CommandMenu |

### Command Menu and Command Palette

- Press `:` in `normal` mode to open the bottom `CommandMenu` bar (`:w`, `:wa`, `:q`, `:wq`, `:bn`, `:bp`, `:bd`, `:new`, `:mkdir`, `:workspace`, `:settings`, `:project-settings`, `:diagnostics`, `:lsp-restart`, `:theme`, `:command-style`, `:preset`, `:leader-only`, `:pane-outline`, `:click-to-insert`, `:wrap`, `:lsp-mode`, `:keybindings`).
- Press `Mod+k` (`Cmd+k` on macOS, `Ctrl+k` on Linux/Windows) to open the configured command surface (`keybindings.commandStyle`: `"menu"` by default, or `"palette"` for the centered fuzzy `CommandPalette` modal).
- Toggle or switch any preference directly from `CommandMenu` or `CommandPalette` without editing JSON by hand.
