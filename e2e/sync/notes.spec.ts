import { describe, it, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { createServer } from 'node:http'
import { once } from 'node:events'
import { stop } from '../common/test-harness.js'
import { waitFor } from '../common/test-utils.js'
import { useSyncFixture, signIn, waitForComment } from './sync.fixtures.js'

// @module-tag e2e sync notes

describe(
  'When signed-in devices synchronize saved-file notes',
  { tags: ['e2e', 'sync', 'notes'] },
  () => {
    const fixture = useSyncFixture()

    it('persists anonymous notes, uploads on sign-in, and resumes offline edits after a restart', async () => {
      // Arrange
      const { a, b, cloud } = fixture()
      await a.identify()
      const path = await a.write('notes.md', 'Plan\n')
      const scope = { notes: { path } }
      const saved = await a.rpc('AddComment', {
        scope,
        body: 'Anonymous feedback',
        anchor: { kind: 'general' },
        requestKey: randomUUID(),
      })
      await a.restart()
      expect((await a.rpc('GetConversation', { scope })).comments[0].id).toBe(saved.comment.id)
      expect((await cloud.pull()).changes).toEqual([])

      // Act
      await signIn(a, cloud.endpoint)
      const uploaded = await waitFor(
        'uploaded notes',
        () => cloud.pull(),
        (value) => value.changes.length === 1,
      )
      const context = uploaded.changes[0].snapshot.context
      const remoteScope = { notes: { path: context.notes_path } }
      b.actor = a.actor
      await signIn(b, cloud.endpoint)
      await waitForComment(b, remoteScope, 'Anonymous feedback')

      // Assert
      expect(JSON.stringify(uploaded)).not.toContain(a.workspace)
      expect(context.notes_path).toContain('/lgtm-unbound/')

      // Act: the second device writes back through the same sync protocol.
      const downloaded = await b.rpc('GetConversation', { scope: remoteScope })
      await b.rpc('Reply', {
        scope: remoteScope,
        threadId: saved.comment.threadId,
        body: 'From device B',
        expectedVersion: downloaded.threads[0].version,
        requestKey: randomUUID(),
      })
      await waitForComment(a, scope, 'From device B')

      // Arrange: disconnect the server and keep editing locally.
      await stop(cloud.child)
      const current = await a.rpc('GetConversation', { scope })
      await a.rpc('Reply', {
        scope,
        threadId: saved.comment.threadId,
        body: 'Written offline',
        expectedVersion: current.threads[0].version,
        requestKey: randomUUID(),
      })
      await a.restart()
      expect((await a.rpc('GetConversation', { scope })).comments).toHaveLength(3)

      // Act
      await cloud.start()
      await waitForComment(b, remoteScope, 'Written offline')
      await b.restart()

      // Assert
      expect((await b.rpc('GetConversation', { scope: remoteScope })).comments).toHaveLength(3)
      expect((await cloud.pull()).changes[0].snapshot.comments).toHaveLength(3)
    })

    it('keeps stale local changes on conflict and isolates authenticated accounts', async () => {
      // Arrange
      const { a, cloud } = fixture()
      await a.identify()
      const path = await a.write('notes.md', 'Plan\n')
      const scope = { notes: { path } }
      const saved = await a.rpc('AddComment', {
        scope,
        body: 'Original',
        anchor: { kind: 'general' },
        requestKey: randomUUID(),
      })
      await signIn(a, cloud.endpoint)
      const uploaded = await waitFor(
        'initial upload',
        () => cloud.pull(),
        (value) => value.changes.length === 1,
      )
      delete a.env.LGTM_SYNC_URL
      delete a.env.LGTM_SYNC_TOKEN
      await a.restart()

      // Act: another device accepts a remote edit while this device is offline.
      const change = uploaded.changes[0]
      change.snapshot.comments[0].body = 'Remote edit'
      change.snapshot.context.revision += 1
      const operation = {
        operation_id: randomUUID(),
        expected_version: change.version,
        snapshot: change.snapshot,
      }
      expect((await cloud.request('/v1/push', operation)).status).toBe(200)
      expect((await cloud.request('/v1/push', operation)).status).toBe(200)
      expect(
        (await cloud.request('/v1/push', { ...operation, operation_id: randomUUID() })).status,
      ).toBe(409)
      await a.rpc('EditComment', {
        scope,
        commentId: saved.comment.id,
        body: 'Local edit',
        expectedVersion: saved.comment.version,
        requestKey: randomUUID(),
      })
      await signIn(a, cloud.endpoint)
      await waitFor('visible sync conflict', async () => a.log.includes('Sync conflict'))

      // Assert
      expect((await a.rpc('GetConversation', { scope })).comments[0].body).toBe('Local edit')
      expect((await cloud.pull()).changes[0].snapshot.comments[0].body).toBe('Remote edit')
      expect((await cloud.pull('test-bob')).changes).toEqual([])
      expect((await cloud.request('/v1/pull', undefined, 'invalid')).status).toBe(401)
      a.env.LGTM_SYNC_TOKEN = 'test-bob'
      await a.restart()
      await waitFor('account binding rejection', async () => a.log.includes('another sync account'))
      expect((await cloud.pull('test-bob')).changes).toEqual([])
    })

    it('retries a durable upload after the cloud commits but its response is lost', async () => {
      // Arrange
      const { a, cloud } = fixture()
      await a.identify()
      const path = await a.write('notes.md', 'Plan\n')
      const scope = { notes: { path } }
      await a.rpc('AddComment', {
        scope,
        body: 'Keep exactly once',
        anchor: { kind: 'general' },
        requestKey: randomUUID(),
      })
      const requests: string[] = []
      const proxy = createServer(async (request, response) => {
        try {
          const chunks: Buffer[] = []
          for await (const chunk of request) chunks.push(Buffer.from(chunk))
          const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString()) : undefined
          const upstream = await cloud.request(request.url!, body)
          const bytes = await upstream.text()
          if (request.url === '/v1/push') {
            requests.push(JSON.stringify(body))
            if (requests.length === 1) {
              response.destroy()
              return
            }
          }
          response.writeHead(upstream.status, { 'content-type': 'application/json' }).end(bytes)
        } catch {
          response.destroy()
        }
      })
      proxy.listen(0, '127.0.0.1')
      await once(proxy, 'listening')
      const address = proxy.address()
      if (!address || typeof address === 'string') throw new Error('Expected TCP address')
      try {
        // Act
        await signIn(a, `http://127.0.0.1:${address.port}`)
        await waitFor('committed upload with lost response', async () => requests.length === 1)
        await a.restart()
        await waitFor('identical retry after restart', async () => requests.length >= 2)
        const result = await cloud.pull()

        // Assert
        expect(requests[1]).toBe(requests[0])
        expect(result.cursor).toBe(1)
        expect(result.changes).toHaveLength(1)
        expect(result.changes[0].snapshot.comments).toHaveLength(1)
      } finally {
        const closed = once(proxy, 'close')
        proxy.close()
        proxy.closeAllConnections()
        await closed
      }
    })
  },
)
