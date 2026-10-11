# LGTM API

The versioned contract is [proto/lgtm/v1/review.proto](proto/lgtm/v1/review.proto).
The service is `lgtm.v1.ReviewService` and uses standard protobuf encoding over gRPC.
Every operation has its own request and response message.
The server also exposes gRPC health checks and server reflection.

| RPC | Request fields | Result |
| --- | --- | --- |
| `GetWorkspace` | None | Current checkout path, reviews, and comments |
| `ListPullRequests` | None | Repository from `origin` and its open GitHub PRs |
| `ListFiles` | `review_id`, `include_unchanged` | Changed files or all checkout files |
| `GetDiff` | `review_id`, `path` | Local HEAD diff or GitHub PR patch |
| `GetFile` | `path` | Source within the configured checkout |
| `GetHistory` | `review_id` | Existing GitHub reviews and discussion |
| `CreateReview` | `kind`, `actor`, optional `body`, `pr`, `repository` | New local draft; GitHub captures the PR head |
| `AddComment` | `path`, `line`, `side`, `body`, `actor`, optional `review_id` | Line comment or standalone note |
| `AddReply` | `thread_id`, `body`, `actor` | Reply with its own author |
| `SetThreadResolved` | `thread_id`, `resolved` | Local resolution or reopening |
| `PromoteThread` | `thread_id`, `review_id` | Move a standalone thread and replies into a draft |
| `UpdateReview` | `review_id`, `body` | Save draft summary |
| `SubmitReview` | `review_id`, `event` | Submit comments, approve, or request changes |

Each response includes a `workspace` snapshot. Lists and file operations include their additional result fields.
Kinds, diff sides, review states, and submission actions are protobuf enums.
Actor kind distinguishes people from agents.
Comments retain their own actor and optional root `thread_id`.
GitHub submission folds replies into one attributed inline comment or summary note.
Thread resolution stays local.

Paths are relative to the configured checkout. Line numbers are positive.
An unspecified diff side defaults to the right side.
GitHub draft creation infers the repository from `origin` when `repository` is absent.
The UIs list open PRs without asking for a repository or PR number.

Invalid input returns `InvalidArgument`. Missing entities return `NotFound`.
Submitted drafts, changed PR heads, and invalid submission states return `FailedPrecondition`.
Upstream failures return `Unavailable`. Database failures return `Internal`.
Canceled requests and expired deadlines retain their standard gRPC codes.

## Generated clients

Run `mise run proto:generate` after changing the schema.
The task installs pinned generators in the ignored `.tmp/proto-tools` directory.
It formats and lints the schema, then generates these files:

- `gen/go/lgtm/v1`: Go messages and gRPC client/server interfaces.
- `gen/go/lgtm/v1/lgtmv1connect`: Connect and gRPC-Web handlers and clients.
- `apps/lgtm-ts-client/gen/lgtm/v1`: TypeScript messages and service descriptors.

The browser uses the generated TypeScript service with protobuf binary requests over Connect.
Neovim uses protobuf JSON at the same typed Connect endpoints through curl.
The HTTP proxy forwards both through the generated Go gRPC client.
No custom gRPC codec or generic Execute RPC remains.

For example, this typed HTTP request reads the current workspace:

```sh
curl -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  --data '{}' \
  http://127.0.0.1:9910/lgtm.v1.ReviewService/GetWorkspace
```

## CLI compatibility adapter

`bin/lgtm rpc` still accepts the previous JSON operation vocabulary on stdin.
`POST /api` remains as a compatibility adapter for existing scripts.
Both dispatch named protobuf RPCs. They are not the wire contract used by either UI.

```sh
printf '%s\n' '{"op":"comment","path":"README.md","line":1,"body":"Clarify setup.","actor":{"name":"Codex","kind":"agent"}}' | bin/lgtm rpc
```

The operation names are `state`, `pulls`, `files`, `diff`, `file`, `history`, `create`, `comment`, `reply`, `resolve`, `promote`, `edit`, and `submit`.
This adapter returns the previous `{state, repoPath, text, files, repository, pulls}` response shape.
