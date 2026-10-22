/**
 * @module-tag e2e
 * @module-tag backend
 * @module-tag notes
 */
import { describe, it, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { copyFile, mkdir, readFile, rename, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'
import { type TestHarness } from '../common/test-harness.js'
import { hash, type Json } from '../common/test-utils.js'
import { line } from './anchors.js'

async function prepareNote(runtime: TestHarness) {
  await runtime.identify()
  const bytes = 'before\r\nselected\r\nafter'
  const file = await runtime.write('notes/one.md', bytes)
  const scope = { notes: { path: file } }
  const request = {
    scope,
    anchor: line(bytes),
    body: 'Explain this case',
    expectedVersion: '0',
    requestKey: randomUUID(),
  }
  const root = (await runtime.rpc('AddComment', request)).comment
  return { bytes, file, scope, request, root }
}

describe('When taking notes while browsing files', () => {
  const fixture = useTestHarness('backend')

  it(
    'User can persist an exact line selection without creating a review',
    { tags: ['smoke'] },
    async () => {
      // Arrange
      const { runtime } = fixture()
      await runtime.identify()
      const bytes = 'before\r\nselected\r\nafter'
      const file = await runtime.write('notes/one.md', bytes)
      const scope = { notes: { path: file } }

      // Act
      const empty = await runtime.rpc('GetConversation', { scope })
      const saved = await runtime.rpc('AddComment', {
        scope,
        anchor: line(bytes),
        body: 'Explain this case',
        expectedVersion: empty.version,
        requestKey: randomUUID(),
      })
      await runtime.restart()
      const conversation = await runtime.rpc('GetConversation', { scope })

      // Assert
      expect(empty.version).toBe('0')
      expect(conversation.comments).toHaveLength(1)
      expect(conversation.comments[0].id).toBe(saved.comment.id)
      expect(conversation.threads[0].anchor).toMatchObject({
        start: '2',
        evidence: { selectedText: 'selected' },
        source: { localFile: { contentHash: hash(bytes) } },
      })
      expect((await runtime.rpc('ListReviews')).reviews ?? []).toEqual([])
    },
  )

  it('Retrying a saved note succeeds even when its source file is temporarily missing', async () => {
    // Arrange
    const { runtime } = fixture()
    const { file, scope, request, root } = await prepareNote(runtime)
    await rm(file)

    // Act
    const replay = await runtime.rpc('AddComment', request)
    const changedPayload = await runtime.response('AddComment', {
      ...request,
      body: 'Different request',
    })

    // Assert
    expect(replay.comment.id).toBe(root.id)
    expect((await runtime.rpc('GetConversation', { scope })).comments).toHaveLength(1)
    expect(changedPayload.status).toBeGreaterThanOrEqual(400)
  })

  it('User can edit a note while stale or mixed versions cannot overwrite it', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, root } = await prepareNote(runtime)
    const edit = {
      scope,
      commentId: root.id,
      body: 'Accepted edit',
      expectedVersion: root.version,
      requestKey: randomUUID(),
    }

    // Act
    const accepted = await runtime.rpc('EditComment', edit)
    const stale = await runtime.response('EditComment', {
      ...edit,
      body: 'Stale edit',
      requestKey: randomUUID(),
    })
    const current = await runtime.rpc('GetConversation', { scope })
    const mixed = root.version.split(':')
    mixed[1] = current.version
    const mixedResult = await runtime.response('EditComment', {
      ...edit,
      expectedVersion: mixed.join(':'),
      requestKey: randomUUID(),
    })
    const replay = await runtime.rpc('EditComment', edit)

    // Assert
    expect(accepted.comment.body).toBe('Accepted edit')
    expect(stale.status).toBeGreaterThanOrEqual(400)
    expect(mixedResult.status).toBeGreaterThanOrEqual(400)
    expect(replay.comment.body).toBe('Accepted edit')
  })
})

describe('When discussing and exporting notes', () => {
  const fixture = useTestHarness('backend')

  it('User can reply, resolve and export a note without changing its stored discussion', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, root } = await prepareNote(runtime)
    let current = await runtime.rpc('GetConversation', { scope })
    await runtime.rpc('Reply', {
      scope,
      threadId: root.threadId,
      body: 'Checked',
      expectedVersion: current.threads[0].version,
      requestKey: randomUUID(),
    })
    current = await runtime.rpc('GetConversation', { scope })

    // Act
    await runtime.rpc('ResolveThread', {
      scope,
      threadId: root.threadId,
      resolved: true,
      expectedVersion: current.threads[0].version,
      requestKey: randomUUID(),
    })
    const before = await runtime.rpc('GetConversation', { scope })
    const exported = await runtime.rpc('Export', { scope })

    // Assert
    for (const text of ['1. **[NOTE]**', 'Explain this case', 'Checked', 'resolved'])
      expect(exported.markdown).toContain(text)
    expect(exported.markdown).not.toContain(root.threadId)
    expect(await runtime.rpc('GetConversation', { scope })).toEqual(before)
  })

  it('User can delete replies and then delete the root without duplicating a retried deletion', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, root } = await prepareNote(runtime)
    let current = await runtime.rpc('GetConversation', { scope })
    const reply = (
      await runtime.rpc('Reply', {
        scope,
        threadId: root.threadId,
        body: 'Reply',
        expectedVersion: current.threads[0].version,
        requestKey: randomUUID(),
      })
    ).comment
    current = await runtime.rpc('GetConversation', { scope })
    const rootVersion = current.comments.find((comment: Json) => comment.id === root.id).version

    // Act
    const protectedRoot = await runtime.response('DeleteComment', {
      scope,
      commentId: root.id,
      expectedVersion: rootVersion,
      requestKey: randomUUID(),
    })
    await runtime.rpc('DeleteComment', {
      scope,
      commentId: reply.id,
      expectedVersion: current.comments.find((comment: Json) => comment.id === reply.id).version,
      requestKey: randomUUID(),
    })
    current = await runtime.rpc('GetConversation', { scope })
    const request = {
      scope,
      commentId: root.id,
      expectedVersion: current.comments[0].version,
      requestKey: randomUUID(),
    }
    const accepted = await runtime.rpc('DeleteComment', request)
    const replay = await runtime.rpc('DeleteComment', request)

    // Assert
    expect(protectedRoot.status).toBeGreaterThanOrEqual(400)
    expect(replay).toEqual(accepted)
    expect((await runtime.rpc('GetConversation', { scope })).comments ?? []).toHaveLength(0)
  })
})

describe('When a note file moves', () => {
  const fixture = useTestHarness('backend')

  it('A copied file cannot claim the original notes location', async () => {
    // Arrange
    const { runtime } = fixture()
    const { file, scope } = await prepareNote(runtime)
    const destination = join(runtime.workspace, 'copy.md')
    await copyFile(file, destination)
    const before = await runtime.rpc('GetConversation', { scope })

    // Act
    const rejected = await runtime.response('RefreshReview', {
      scope,
      expectedVersion: before.version,
      sourceChanges: [{ locationMoved: { previousPath: file, currentPath: destination } }],
      requestKey: randomUUID(),
    })

    // Assert
    expect(rejected.status).toBeGreaterThanOrEqual(400)
    expect(await runtime.rpc('GetConversation', { scope })).toEqual(before)
  })

  it('User keeps original anchors and can add notes at the new location after restart', async () => {
    // Arrange
    const { runtime } = fixture()
    const { bytes, file, scope, root } = await prepareNote(runtime)
    const destination = join(runtime.workspace, 'moved-notes/two.md')
    await mkdir(join(runtime.workspace, 'moved-notes'))
    const before = await runtime.rpc('GetConversation', { scope })
    await rename(file, destination)
    const request = {
      scope,
      expectedVersion: before.version,
      sourceChanges: [{ locationMoved: { previousPath: file, currentPath: destination } }],
      requestKey: randomUUID(),
    }

    // Act
    const accepted = await runtime.rpc('RefreshReview', request)
    const replay = await runtime.rpc('RefreshReview', request)
    const movedScope = { notes: { path: destination } }
    const moved = await runtime.rpc('GetConversation', { scope: movedScope })
    await runtime.rpc('AddComment', {
      scope: movedScope,
      anchor: line(bytes),
      body: 'Note at destination',
      expectedVersion: moved.version,
      requestKey: randomUUID(),
    })
    await runtime.restart()
    const restored = await runtime.rpc('GetConversation', { scope: movedScope })

    // Assert
    expect(replay).toEqual(accepted)
    expect(restored.comments).toHaveLength(2)
    expect(restored.comments.map((comment: Json) => comment.id)).toContain(root.id)
    expect(restored.threads.find((thread: Json) => thread.id === root.threadId)).toMatchObject({
      sourcePath: file,
      currentPath: destination,
      anchor: before.threads[0].anchor,
    })
  })
})

describe('When a note file is deleted and restored', () => {
  const fixture = useTestHarness('backend')

  it('User keeps file-level feedback orphaned until the original bytes return', async () => {
    // Arrange
    const { runtime } = fixture()
    await runtime.identify()
    const bytes = 'original file\n'
    const file = await runtime.write('file-note.md', bytes)
    const scope = { notes: { path: file } }
    const saved = (
      await runtime.rpc('AddComment', {
        scope,
        body: 'File feedback',
        anchor: { kind: 'file', source: { localFile: { contentHash: hash(bytes) } }, evidence: {} },
        expectedVersion: '0',
        requestKey: randomUUID(),
      })
    ).comment
    const original = await runtime.rpc('GetConversation', { scope })
    const thread = original.threads[0]
    await rm(file)
    const deleted = await runtime.rpc('RefreshReview', {
      scope,
      expectedVersion: original.version,
      sourceChanges: [{ deleted: { threadId: thread.id, path: 'file-note.md' } }],
      requestKey: randomUUID(),
    })
    await runtime.write('file-note.md', 'unrelated replacement\n')
    const restore = (content: string) => ({
      scope,
      expectedVersion: deleted.version,
      sourceChanges: [
        {
          restored: {
            threadId: thread.id,
            path: 'file-note.md',
            source: { localFile: { contentHash: hash(content) } },
          },
        },
      ],
      requestKey: randomUUID(),
    })

    // Act
    const mismatch = await runtime.response('RefreshReview', restore('unrelated replacement\n'))
    const orphaned = await runtime.rpc('GetConversation', { scope })
    await runtime.write('file-note.md', bytes)
    await runtime.rpc('RefreshReview', restore(bytes))
    const restored = await runtime.rpc('GetConversation', { scope })

    // Assert
    expect(thread.anchor).toMatchObject({
      kind: 'file',
      source: { localFile: { contentHash: hash(bytes) } },
    })
    expect(thread.anchor.start).toBeFalsy()
    expect(mismatch.status).toBeGreaterThanOrEqual(400)
    expect(orphaned.threads[0].orphanReason).toBe('file_deleted')
    expect(restored.comments[0].id).toBe(saved.id)
    expect(restored.threads[0].anchor).toEqual(thread.anchor)
    expect(restored.threads[0].orphanReason).toBeFalsy()
    expect(await readFile(file, 'utf8')).toBe(bytes)
  })
})
