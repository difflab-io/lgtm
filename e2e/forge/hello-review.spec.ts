/**
 * @module-tag e2e
 * @module-tag forge
 */
import { describe, it, expect } from 'vitest'
import { readdir, writeFile, readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'

describe(
  'When calling the hello-review forge service',
  { tags: ['e2e', 'forge', 'hello-review'] },
  () => {
    const fixture = useTestHarness('forge')

    it('should_return_a_greeting_for_the_supplied_name', { tags: ['smoke'] }, async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const reply = await runtime.rpc('HelloReview', { name: 'Backend' })

      // Assert
      expect(reply).toEqual({ message: 'Hello, Backend!' })
    })

    it('should_normalize_empty_and_padded_names_without_touching_legacy_reviews_sqlite', async () => {
      // Arrange
      const { runtime } = fixture()
      const oldDatabase = join(runtime.home, 'reviews.sqlite')
      await writeFile(oldDatabase, 'existing database stays untouched')

      // Act
      const empty = await runtime.rpc('HelloReview', { name: '' })
      const padded = await runtime.rpc('HelloReview', { name: '  Zoë  ' })
      await runtime.restart()

      // Assert
      expect(empty.message).toBe('Hello, World!')
      expect(padded.message).toBe('Hello, Zoë!')
      expect(await readFile(oldDatabase, 'utf8')).toBe('existing database stays untouched')
      const dbFiles = (await readdir(runtime.home)).filter((name) => /sqlite|\.db$/.test(name))
      expect(dbFiles).toContain('reviews.sqlite')
      expect(dbFiles).toContain('lgtm.db')
    })

    it('should_reject_removed_review_methods_instead_of_running_old_workflows', async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const response = await runtime.response('StartReview', {})

      // Assert
      expect(response.status).not.toBe(200)
    })
  },
)
