import { randomUUID } from 'node:crypto'
import { join } from 'node:path'
import { type TestHarness } from '../common/test-harness.js'
import { hash } from '../common/test-utils.js'
import { line } from './anchors.js'

export async function prepareDirectory(runtime: TestHarness) {
  await runtime.identify()
  const bytes = 'before\nselected\nafter\n'
  const plans = join(runtime.workspace, '.difflab/plan/example')
  await runtime.write('.gitignore', '.difflab/\n')
  await runtime.write('.difflab/plan/example/PLAN.md', bytes)
  await runtime.write('.difflab/plan/example/two.md', 'second file\n')
  const target = { directory: { path: plans } }
  const review = (await runtime.rpc('StartReview', { target, requestKey: randomUUID() })).review
  const scope = { reviewId: review.id }
  return { runtime, bytes, plans, target, review, scope }
}

export async function prepareDirectoryDiscussion(runtime: TestHarness) {
  const current = await prepareDirectory(runtime)
  const root = (
    await runtime.rpc('AddComment', {
      scope: current.scope,
      anchor: line(current.bytes, 'PLAN.md'),
      body: 'Handle this case',
      expectedVersion: current.review.version,
      requestKey: randomUUID(),
    })
  ).comment
  let snapshot = await runtime.rpc('GetConversation', { scope: current.scope })
  await runtime.rpc('Reply', {
    scope: current.scope,
    threadId: root.threadId,
    body: 'Keep the fallback',
    expectedVersion: snapshot.threads[0].version,
    requestKey: randomUUID(),
  })
  snapshot = await runtime.rpc('GetConversation', { scope: current.scope })
  await runtime.rpc('AddComment', {
    scope: current.scope,
    anchor: { kind: 'file', path: 'two.md' },
    body: 'Explain this file',
    expectedVersion: snapshot.version,
    requestKey: randomUUID(),
  })
  return { ...current, root }
}

export async function prepareStagedReview(runtime: TestHarness) {
  await runtime.identify()
  await runtime.git('init', '-q', '-b', 'main')
  const path = 'src/[x]`Ω.rs'
  const old = 'before\nselected old\n'
  const next = 'before\nselected new\n'
  await runtime.write(path, old)
  await runtime.write('other.md', 'file feedback target\n')
  const base = await runtime.commit('base')
  await runtime.git('checkout', '-qb', 'feature')
  await runtime.write(path, next)
  await runtime.git('add', '.')
  const target = { localGit: { path: runtime.workspace } }
  const review = (
    await runtime.rpc('StartReview', {
      target,
      comparison: { staged: { baseCommit: base } },
      requestKey: randomUUID(),
    })
  ).review
  const scope = { reviewId: review.id }
  const root = (
    await runtime.rpc('AddComment', {
      scope,
      body: 'Handle this failure before returning.\n\n```rust\nreturn recover(error);\n```',
      anchor: {
        kind: 'line',
        path,
        side: 'old',
        start: '2',
        end: '2',
        source: { commit: { sha: base, contentHash: hash(old) } },
        evidence: { selectedText: 'selected old', contextBefore: 'before' },
      },
      expectedVersion: review.version,
      requestKey: randomUUID(),
    })
  ).comment
  return { runtime, path, old, next, base, target, review, scope, root }
}

export async function prepareBranchDiscussion(runtime: TestHarness) {
  const current = await prepareStagedReview(runtime)
  const snapshot = await runtime.rpc('GetConversation', { scope: current.scope })
  const head = await runtime.commit('reviewed change')
  const replacement = {
    target: current.target,
    comparison: { branch: { baseCommit: current.base, headCommit: head } },
    resumePolicy: 'RESUME_POLICY_REPLACE_LOCAL',
    expectedReviewId: current.review.id,
    expectedVersion: snapshot.version,
    requestKey: randomUUID(),
  }
  const branch = (await runtime.rpc('StartReview', replacement)).review
  const scope = { reviewId: branch.id }
  let conversation = await runtime.rpc('GetConversation', { scope })
  await runtime.rpc('Reply', {
    scope,
    threadId: current.root.threadId,
    body: 'Add a regression test for the timeout case.',
    expectedVersion: conversation.threads[0].version,
    requestKey: randomUUID(),
  })
  conversation = await runtime.rpc('GetConversation', { scope })
  const fileComment = (
    await runtime.rpc('AddComment', {
      scope,
      body: 'Keep this explanation in the replacement.',
      anchor: {
        kind: 'file',
        path: 'other.md',
        source: { commit: { sha: head, contentHash: hash('file feedback target\n') } },
        evidence: {},
      },
      expectedVersion: conversation.version,
      requestKey: randomUUID(),
    })
  ).comment
  conversation = await runtime.rpc('GetConversation', { scope })
  const general = (
    await runtime.rpc('AddComment', {
      scope,
      body: 'Check the whole failure path.',
      expectedVersion: conversation.version,
      requestKey: randomUUID(),
    })
  ).comment
  return { ...current, scope, branch, replacement, head, fileComment, general }
}
