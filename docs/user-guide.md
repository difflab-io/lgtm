# User guide

LGTM currently provides a hello-review demonstration while its review model is rebuilt.

Start the bundled application with `lgtm up`, or use `lgtm ui` to start/reuse it and open the browser. Submit a name in the page to receive a response from the backend. Use `lgtm ui --no-open` to print the URL without opening a browser, `lgtm status --json` to inspect the instance and `lgtm down` to stop it.

`lgtm review list` calls the same review RPC and returns labeled demo data. The `commit`, `push`, `review new`, `review close` and `comment` commands expose the proposed shape but return unimplemented errors without modifying the checkout or upstream forge. Reviews, comments and agent identities are not saved; old database files are unused and left intact.

The Neovim plugin loads its pinned dependencies but has no review commands or review UI. See the [plugin setup guide](../apps/lgtm.nvim/README.md).

For exact flags, JSON output and endpoint configuration, see [CLI usage](../apps/lgtm-cli/README.md). For development setup, see [development](development.md).
