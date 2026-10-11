import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ReviewService,
  ActorKind,
  ReviewKind,
  ReviewStatus,
  DiffSide,
  ReviewEvent,
  type Actor as ProtoActor,
  type Workspace,
} from "./gen/lgtm/v1/review_pb";
export * as protocol from "./gen/lgtm/v1/review_pb";
export type Actor = { name: string; kind: "human" | "agent" };
export type Review = {
  id: string;
  kind: "local" | "github";
  repo: string;
  pr: number;
  head: string;
  status: string;
  body: string;
  actor: Actor;
  remoteId: number;
};
export type Comment = {
  threadId?: string;
  resolved?: boolean;
  id: string;
  path: string;
  line: number;
  side: "LEFT" | "RIGHT";
  body: string;
  actor: Actor;
  reviewId: string;
};
export type ChangedFile = { path: string; status: string };
export type State = { reviews: Review[]; comments: Comment[] };
export type Request = {
  op:
    | "pulls"
    | "state"
    | "create"
    | "comment"
    | "promote"
    | "submit"
    | "edit"
    | "diff"
    | "file"
    | "history"
    | "files"
    | "reply"
    | "resolve";
  id?: string;
  kind?: string;
  repo?: string;
  pr?: number;
  path?: string;
  line?: number;
  side?: string;
  body?: string;
  actor?: Actor;
  reviewId?: string;
  event?: string;
};
export type Response = {
  state: State;
  text?: string;
  files?: ChangedFile[];
  repoPath?: string;
  repository?: string;
  pulls?: { number: number; title: string; draft: boolean }[];
};

function actorFromProto(actor?: ProtoActor): Actor {
  return {
    name: actor?.name || "",
    kind: actor?.kind === ActorKind.AGENT ? "agent" : "human",
  };
}
function snapshot(workspace?: Workspace): Response {
  return {
    repoPath: workspace?.repoPath,
    state: {
      reviews: (workspace?.state?.reviews || []).map((r) => ({
        id: r.id,
        kind: r.kind === ReviewKind.GITHUB ? "github" : "local",
        repo: r.repo,
        pr: r.pr,
        head: r.head,
        status:
          r.status === ReviewStatus.DRAFT ? "draft" : ReviewStatus[r.status],
        body: r.body,
        actor: actorFromProto(r.actor),
        remoteId: Number(r.remoteId),
      })),
      comments: (workspace?.state?.comments || []).map((c) => ({
        id: c.id,
        threadId: c.threadId || undefined,
        resolved: c.resolved,
        path: c.path,
        line: c.line,
        side: c.side === DiffSide.LEFT ? "LEFT" : "RIGHT",
        body: c.body,
        actor: actorFromProto(c.actor),
        reviewId: c.reviewId,
      })),
    },
  };
}
// The UI adapter preserves view-model strings. Every wire request uses the
// generated service descriptor and protobuf binary encoding over Connect.
export async function execute(q: Request): Promise<Response> {
  const client = createClient(
    ReviewService,
    createConnectTransport({
      baseUrl: globalThis.location.origin,
      useBinaryFormat: true,
    }),
  );
  const actor = q.actor
    ? {
        name: q.actor.name,
        kind: q.actor.kind === "agent" ? ActorKind.AGENT : ActorKind.HUMAN,
      }
    : undefined;
  switch (q.op) {
    case "state": {
      const r = await client.getWorkspace({});
      return snapshot(r.workspace);
    }
    case "pulls": {
      const r = await client.listPullRequests({});
      return {
        ...snapshot(r.workspace),
        repository: r.repository,
        pulls: r.pulls.map((p) => ({
          number: p.number,
          title: p.title,
          draft: p.draft,
        })),
      };
    }
    case "files": {
      const r = await client.listFiles({
        reviewId: q.id,
        includeUnchanged: q.kind === "all",
      });
      return {
        ...snapshot(r.workspace),
        files: r.files.map((f) => ({ path: f.path, status: f.status })),
      };
    }
    case "diff": {
      const r = await client.getDiff({ reviewId: q.id, path: q.path });
      return { ...snapshot(r.workspace), text: r.text };
    }
    case "file": {
      const r = await client.getFile({ path: q.path });
      return { ...snapshot(r.workspace), text: r.text };
    }
    case "history": {
      const r = await client.getHistory({ reviewId: q.id });
      return { ...snapshot(r.workspace), text: r.text };
    }
    case "create": {
      const r = await client.createReview({
        kind: q.kind === "github" ? ReviewKind.GITHUB : ReviewKind.LOCAL,
        actor,
        body: q.body,
        pr: q.pr,
        repository: q.repo,
      });
      return snapshot(r.workspace);
    }
    case "comment": {
      const r = await client.addComment({
        path: q.path,
        line: q.line,
        side: q.side === "LEFT" ? DiffSide.LEFT : DiffSide.RIGHT,
        body: q.body,
        actor,
        reviewId: q.reviewId,
      });
      return snapshot(r.workspace);
    }
    case "reply": {
      const r = await client.addReply({ threadId: q.id, body: q.body, actor });
      return snapshot(r.workspace);
    }
    case "resolve": {
      const r = await client.setThreadResolved({
        threadId: q.id,
        resolved: q.event !== "REOPEN",
      });
      return snapshot(r.workspace);
    }
    case "promote": {
      const r = await client.promoteThread({
        threadId: q.id,
        reviewId: q.reviewId,
      });
      return snapshot(r.workspace);
    }
    case "edit": {
      const r = await client.updateReview({ reviewId: q.id, body: q.body });
      return snapshot(r.workspace);
    }
    case "submit": {
      const event = {
        COMMENT: ReviewEvent.COMMENT,
        APPROVE: ReviewEvent.APPROVE,
        REQUEST_CHANGES: ReviewEvent.REQUEST_CHANGES,
      }[q.event || ""];
      if (event === undefined)
        throw new Error("Choose a review submission action");
      const r = await client.submitReview({ reviewId: q.id, event });
      return snapshot(r.workspace);
    }
  }
}
