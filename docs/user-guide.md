# LGTM User Guide

## Modal Editing and Keybindings

LGTM uses modal editing powered by Helical UI. The header displays the active mode (`normal`, `insert`, `visual`, `visual_line`, or `goto`).

### Editing Presets

Switch between three modal presets in `config.json` or from the command menu:

| Preset | Philosophy | Line Selection | Undo / Redo |
| --- | --- | --- | --- |
| `vim` (default) | Operator + motion (`dw`, `ciw`, `yy`) | `V` (`visual_line`) | `u` / `Ctrl+r` |
| `kakoune` | Selection first, action second (`wd`, `wc`) | `x` (extend line) | `u` / `U` |
| `helix` | Selection first + `gd` goto and `Space` pickers | `x` (extend line) | `u` / `U` |

### Core Motions and Operators

| Key | Action |
| --- | --- |
| `h`, `j`, `k`, `l` | Move cursor left, down, up, right |
| `w`, `b`, `e` | Move by word start, previous word, word end |
| `0`, `^`, `$` | Move to line start, first non-whitespace character, line end |
| `gg`, `G` | Jump to document start or end (or `<count>G` to line) |
| `f<char>`, `t<char>` | Jump to or before `<char>` on the current line |
| `%` | Jump to matching bracket |
| `i`, `a`, `I`, `A`, `o`, `O` | Enter `insert` mode |
| `Escape` | Return to `normal` mode |
| `v`, `V` | Enter character or line `visual` selection mode |
| `d`, `c`, `y`, `p`, `P` | Delete, change, yank, or paste selection |
| `gc`, `gcc` | Toggle line comments |
| `f` (normal) | Activate two-character Flash jump labels across visible elements |

### Leader Shortcuts (`Space`)

Press `Space` in `normal` mode to open the WhichKey popup:

| Sequence | Action |
| --- | --- |
| `Space f f` | Focus file explorer |
| `Space f n` / `Space f N` | Create file or folder in explorer |
| `Space f s` | Save active file |
| `Space f c` | Open global settings (`~/.lgtm/config.json`) |
| `Space f C` | Open project settings (`.lgtm/config.json`) |
| `Space b n` / `Space b p` / `Space b d` | Next tab, previous tab, close active tab |
| `Space w h` / `Space w l` / `Space w w` | Focus workspace pane, focus editor pane, or cycle panes |
| `Space w e` | Toggle sidebar |
| `Space l d` / `Space l r` / `Space l g` / `Space l k` | Open diagnostics, restart LSP server, go to definition, show hover |
| `Space s t` / `Space s k` / `Space s c` | Switch theme, keybind preset, or command style |
| `Space s w` / `Space s l` / `Space s p` / `Space s i` | Toggle line wrapping, leader-only mode, pane outline, or click-to-insert |
| `Space ?` | Open the interactive WhichKey hotkey explorer |

### Command Menu and Command Palette

- Press `:` in `normal` mode to open the bottom `CommandMenu` bar (`:w`, `:q`, `:wq`, `:e`, `:bn`, `:bp`, `:bd`, `:theme`, `:preset`, `:command-style`, `:wrap`, `:diagnostics`, `:lsp-restart`).
- Press `Mod+k` (`Cmd+k` on macOS, `Ctrl+k` on Linux/Windows) to open the configured command surface (`keybindings.commandStyle`: `"menu"` by default, or `"palette"` for the centered fuzzy `CommandPalette` modal).
- Toggle or switch any preference directly from `CommandMenu` or `CommandPalette` without editing JSON by hand.
