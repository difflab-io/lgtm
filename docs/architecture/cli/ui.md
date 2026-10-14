# UI module architecture

## Overview

The UI module builds review URLs and opens them in the browser.

## Requirement

### Functional

- Ensure the shared server is ready before opening its UI.
- Pass the supplied path through the `path` query parameter.
- Print the review URL when browser launch is disabled.
- Open the base UI URL for `up`.

### Non-Functional

- Preserve UTF-8 paths, spaces, and reserved URL characters.
- Keep filesystem storage and process management in their own modules.
- Keep the printed URL usable when browser launch fails.

## Design

### Components and Responsibilities

![Review URL opening](diagrams/ui.svg)

- `open_review_ui_at_path` builds the URL for a supplied path.
- `daemon::ensure_running` starts or reuses the shared server.
- `reqwest::Url` encodes the query parameter.
- `webbrowser` opens the URL through the operating system.
- The web page reads `path` after it loads.

### Data and Control Flow

The CLI supplies its absolute current directory. The UI module checks that the path is valid UTF-8, obtains a ready instance, and appends the encoded path to its base URL. It prints the URL and optionally opens it.

Opening another review changes the browser URL. It reuses the same server and does not write a review record.

### Interfaces and Contracts

```rust
pub async fn open_review_ui_at_path(
    path: &Path,
    port: u16,
    open_browser: bool,
    store: &Store,
) -> Result<()>
```

`path` is the value sent to the frontend. The function does not resolve it, read it, or grant filesystem access. The CLI supplies the current directory.

`port` applies when a new server starts. A running instance keeps its existing port. With `open_browser = false`, the function still ensures readiness and prints the URL.

For a path such as `/Users/dev/project`, the URL is `http://127.0.0.1:8080/?path=%2FUsers%2Fdev%2Fproject`.

`open(url: &str)` is the internal browser helper. It prints a warning if the operating system cannot open the browser.

## Implementation

### URL construction

The function uses `query_pairs_mut().append_pair("path", path)` rather than joining raw strings. This preserves spaces, `&`, `+`, `#`, and non-ASCII text as one query value. Invalid UTF-8 returns an error before server startup.

### Browser behavior

The function asks the operating system to open the URL. The browser decides whether that uses an existing window or a new tab. Browser launch failure leaves the URL printed and returns command success after server readiness.

### Frontend handoff

The Svelte page reads `window.location.href` on mount and displays the decoded `path`. Neovim can use `review --no-open` to obtain the URL, or discover the API through `status --json`.

## References

- [UI implementation](../../../apps/lgtm-cli/src/ui.rs).
- [CLI command dispatch](../../../apps/lgtm-cli/src/main.rs).
- [Web page](../../../apps/lgtm-web/src/routes/+page.svelte).
- [Lifecycle tests](../../../apps/lgtm-cli/tests/lifecycle.rs).
- [Daemon module](daemon.md).
