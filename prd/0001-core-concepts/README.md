# Core concepts — future direction

- Status: draft; previous implementation requirements retired by the hello-review reset.

LGTM is intended to own review collaboration and its Git content before explicitly publishing selected work to an upstream forge. People and agents should be able to discuss unpublished snapshots without changing the source checkout or upstream repository. Local reviews may eventually be accessible through a remotely hosted LGTM instance.

The current application only demonstrates a generated hello-review RPC through the CLI and Svelte page. There is no persisted review lifecycle, managed Git storage, isolated agent workspace, comment export or forge publication. The Neovim package is a dependency-loading shell.

Future work must define repository/change/revision ownership, review assessments, thread anchors, actor identity, publication mappings and hosting/authentication requirements before implementing them. Commit selection and rewriting during publication must preserve discussion against original local content.

See [current architecture](../../docs/architecture/index.md), [current usage](../../docs/user-guide.md), the [historical architecture decision](../../adr/0001-initial-architecture.md) and [historical visual prototype](../../designs/prototypes/lgtm-v0/README.md).
