import { randomUUID } from 'node:crypto'
import { type TestHarness } from '../common/test-harness.js'
import { type Json } from '../common/test-utils.js'
import type { GithubFixture } from '../common/github.js'

/** Establish a consumer-visible review and exact sources through public RPCs. */
export async function prepareReview(
  runtime: TestHarness,
  github: GithubFixture,
  identity: Json = { name: 'E2E user', email: 'e2e@example.test' },
) {
  await runtime.identify(identity)
  const target = {
    pullRequest: { repositoryUrl: github.repositoryUrl, number: String(github.number) },
  }
  const review = (await runtime.rpc('StartReview', { target, requestKey: randomUUID() })).review
  const scope = { reviewId: review.id }
  const diff = (await runtime.rpc('GetDiff', { target, comparison: review.comparison })).diff
  const file = await runtime.rpc('GetFile', { diff, path: github.path })
  const anchor = {
    kind: 'line',
    snapshotId: review.snapshot.id,
    path: github.path,
    side: 'new',
    start: '2',
    end: '2',
    source: file.newSide.source,
    evidence: {
      selectedText: 'changed',
      contextBefore: 'first',
      contextAfter: 'stable context 1\nstable context 2\nstable context 3',
    },
  }
  return { runtime, github, target, review, scope, diff, file, anchor }
}

/** Capture the current guard while preserving the caller's explicit comment content and anchor. */
export async function saveDraft(runtime: TestHarness, scope: Json, body: string, anchor: Json) {
  const current = await runtime.rpc('GetConversation', { scope })
  const review = (await runtime.rpc('GetReview', { reviewId: scope.reviewId })).review
  return runtime.rpc('AddComment', {
    scope,
    body,
    anchor: { ...anchor, snapshotId: anchor.snapshotId ?? review.snapshot.id },
    expectedVersion: current.version,
    requestKey: randomUUID(),
  })
}

export async function refreshReview(runtime: TestHarness, scope: Json) {
  const current = await runtime.rpc('GetConversation', { scope })
  return runtime.rpc('RefreshReview', {
    scope,
    expectedVersion: current.version,
    requestKey: randomUUID(),
  })
}

export async function submitReview(
  runtime: TestHarness,
  reviewId: string,
  outcome = 'comment',
  body = 'Final feedback',
) {
  const review = (await runtime.rpc('GetReview', { reviewId })).review
  const request = {
    reviewId,
    outcome,
    body,
    expectedVersion: review.version,
    expectedBase: review.comparison.pullRequest.baseCommit,
    expectedHead: review.comparison.pullRequest.headCommit,
    requestKey: randomUUID(),
  }
  return { request, response: await runtime.response('SubmitReview', request) }
}

/** Pending comments are exposed through their pending review, not the published-comment endpoint. */
export async function readPendingComment(
  runtime: TestHarness,
  github: GithubFixture,
  reviewId: string,
  commentId: string,
): Promise<Json> {
  const review = (await runtime.rpc('GetReview', { reviewId })).review
  const comments = await github.api<Json[]>(
    `repos/${github.repository}/pulls/${github.number}/reviews/${review.forgeReviewId}/comments`,
    'GET',
    undefined,
    { GH_TOKEN: runtime.env.GH_TOKEN },
  )
  const comment = comments.find((comment) => String(comment.id) === commentId)
  if (!comment)
    throw new Error(`Draft ${commentId} absent from pending review ${review.forgeReviewId}`)
  const remote = await github.api(
    'graphql',
    'POST',
    {
      query:
        'query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{path diffSide subjectType line isResolved comments(first:100){nodes{databaseId}}}}}}}',
      variables: {
        owner: github.repository.split('/')[0],
        name: github.repository.split('/')[1],
        number: github.number,
      },
    },
    { GH_TOKEN: runtime.env.GH_TOKEN },
  )
  const thread = remote.data.repository.pullRequest.reviewThreads.nodes.find((thread: Json) =>
    thread.comments.nodes.some((item: Json) => String(item.databaseId) === commentId),
  )
  return { ...comment, thread }
}
