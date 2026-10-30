# LGTM user guide

Use the [CLI](../apps/lgtm-cli/README.md) for notes, local reviews, GitHub reviews and exports. It provides the current review interface. The browser review interface remains planned.

The previous Neovim UI and business logic have been removed for a rewrite. Its commands, keymaps and backend integration are unavailable. The [plugin shell](../apps/lgtm.nvim/README.md) retains package loading, an inert setup hook and [editor help](../apps/lgtm.nvim/doc/lgtm.txt).

The [E2E guide](../e2e/README.md) describes the retained editor fixtures, command execution and diagnostics for developing the replacement plugin.
