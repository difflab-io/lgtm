import { isolatedEnv } from '../common/test-utils.js'
/**
 * @module-tag e2e
 * @module-tag cli
 * @module-tag notes
 */
import { describe, it, expect } from 'vitest'
import { mkdir, rename, rm, access } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'
import { binary, command } from '../common/test-harness.js'

describe('When the backend is unavailable', () => {
  const fixture = useTestHarness('cli')
  it('User gets a JSON error without starting a backend', async () => {
    // Arrange
    const { runtime: r } = fixture()
    const home = join(r.directory, 'offline')

    await mkdir(home)

    // Act
    const result = await command(binary('lgtm'), ['session', 'list', '--json'], {
      env: isolatedEnv(home),
    })

    // Assert
    expect(result.code).not.toBe(0)
    expect(JSON.parse(result.stdout).error.message).toContain('backend')
    await expect(access(join(home, 'server.port'))).rejects.toThrow()
  })
})

describe('When taking notes while browsing files', () => {
  const fixture = useTestHarness('cli')
  it(
    'User can persist exact selections, retry writes and delete a note after its file disappears',
    { tags: ['smoke'] },
    async () => {
      // Arrange
      const { runtime: r } = fixture()
      await r.git('init', '-q', '-b', 'main')
      const file = await r.write('notes ü.md', 'before\r\nselected\r\nafter')

      // Arrange
      const args = [
        'comments',
        'add',
        '--notes',
        file,
        '--file',
        'notes ü.md',
        '--line',
        '2',
        '--body',
        'Explain the fallback',
        '--expected-version',
        '0',
        '--request-key',
        'note-once',
      ]

      // Act
      const saved = await r.json(args)
      const replayedComment = await r.json(args)

      // Assert
      expect(replayedComment.comment.id).toBe(saved.comment.id)
      const notes = await r.json(['comments', 'list', '--notes', file])
      expect(notes.comments).toHaveLength(1)
      expect(notes.threads[0].anchor.evidence.selectedText).toBe('selected')
      expect(notes.threads[0].anchor.source.localFile.contentHash).toMatch(/^sha256:/)
      expect((await r.json(['session', 'list', '--all'])).reviews).toEqual([])
      expect((await r.json(['session', 'export', '--notes', file])).markdown).toContain(
        '1. **[NOTE]**',
      )

      // Act
      await rm(file)

      // Arrange
      const deletion = [
        'comments',
        'delete',
        saved.comment.id,
        '--notes',
        file,
        '--expected-version',
        notes.comments[0].version,
        '--request-key',
        'delete-once',
      ]

      // Act
      const acceptedDeletion = await r.json(deletion)
      const replayedDeletion = await r.json(deletion)

      // Assert
      expect(acceptedDeletion).toEqual(replayedDeletion)
      expect((await r.json(['comments', 'list', '--notes', file])).comments).toEqual([])

      // Act
    },
  )
})

describe('When a note file moves', () => {
  const fixture = useTestHarness('cli')
  it('User keeps the note identity at its new location and can retry the move', async () => {
    // Arrange
    const { runtime: r } = fixture()
    const original = await r.write('before.md', 'before\nselected\nafter\n'),
      destination = join(r.workspace, 'after.md')
    const movedSaved = (
      await r.json([
        'comments',
        'add',
        '--notes',
        original,
        '--file',
        'before.md',
        '--line',
        '2',
        '--body',
        'Move this note',
      ])
    ).comment
    const before = await r.json(['comments', 'list', '--notes', original])
    await rename(original, destination)

    // Arrange
    const move = [
      'session',
      'refresh',
      '--notes',
      destination,
      '--moved-from',
      original,
      '--expected-version',
      before.version,
      '--request-key',
      'file-move',
    ]

    // Act
    const accepted = await r.json(move)
    const replayedMove = await r.json(move)

    // Assert
    expect(replayedMove).toEqual(accepted)
    expect((await r.json(['comments', 'list', '--notes', destination])).comments[0].id).toBe(
      movedSaved.id,
    )
  })
})
