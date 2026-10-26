import { describe, it, expect } from 'vitest'
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { request } from 'node:http'
import { setTimeout as delay } from 'node:timers/promises'
import { RecordedGithubDriver } from './github-recording.js'
import { LiveGithubDriver } from './github-driver.js'
import { CassetteReplay, type Cassette, type Exchange } from './github-cassette.js'

// fixtures --------------------------------------------------------------------

const read = (path: string, actor = 'primary'): Exchange['request'] => ({
  actor,
  method: 'GET',
  path,
  body: null,
})
const write = (body: Record<string, unknown>): Exchange['request'] => ({
  actor: 'primary',
  method: 'POST',
  path: '/graphql',
  body,
})
function cassette(...exchanges: [Exchange['request'], Record<string, unknown>][]): Cassette {
  return {
    version: 1,
    fixture: {
      repository: 'owner/repo',
      repositoryUrl: 'https://github.com/owner/repo',
      run: 'fixture',
      nodeId: 'PR_1',
      base: 'base',
      head: 'head',
      initialHead: 'head',
      number: 1,
    },
    responses: exchanges.map(([, body]) => ({ status: 200, headers: {}, body })),
    exchanges: exchanges.map(([request], response) => ({ request, response })),
  }
}

// tests -----------------------------------------------------------------------

describe('When replaying recorded GitHub requests', () => {
  it('should_bind_runtime_UUIDs_consistently_without_ignoring_comment_content', () => {
    // Arrange
    const original = '11111111-1111-4111-8111-111111111111',
      actual = '22222222-2222-4222-8222-222222222222'
    const replay = new CassetteReplay(
      cassette(
        [
          write({ query: 'mutation', body: `Concern <!-- ${original} -->` }),
          { body: `Concern <!-- ${original} -->` },
        ],
        [write({ query: 'mutation', body: `Reply to ${original}` }), { saved: true }],
      ),
    )
    // Act and Assert
    expect(() =>
      replay.respond(write({ query: 'mutation', body: `Wrong text <!-- ${actual} -->` })),
    ).toThrow('Unrecorded')
    expect(
      replay.respond(write({ query: 'mutation', body: `Concern <!-- ${actual} -->` })).body,
    ).toEqual({ body: `Concern <!-- ${actual} -->` })
    expect(() =>
      replay.respond(write({ query: 'mutation', body: `Reply to ${original}` })),
    ).toThrow('Unrecorded')
    replay.respond(write({ query: 'mutation', body: `Reply to ${actual}` }))
    expect(() => replay.verify()).not.toThrow()
  })

  it('should_keep_recorded_fixture_paths_fixed', () => {
    // Arrange
    const fixed = '11111111-1111-4111-8111-111111111111',
      wrong = '22222222-2222-4222-8222-222222222222'
    const saved = cassette([
      write({ query: 'mutation', path: `lgtm-e2e-${fixed}.txt` }),
      { saved: true },
    ])
    saved.fixture.run = `lgtm-e2e-${fixed}`
    const replay = new CassetteReplay(saved)
    // Act and Assert
    expect(() =>
      replay.respond(write({ query: 'mutation', path: `lgtm-e2e-${wrong}.txt` })),
    ).toThrow('Unrecorded')
    replay.respond(write({ query: 'mutation', path: `lgtm-e2e-${fixed}.txt` }))
    expect(() => replay.verify()).not.toThrow()
  })

  it('should_reuse_prior_reads_without_exposing_future_states_before_writes', () => {
    // Arrange
    const replay = new CassetteReplay(
      cassette(
        [read('/pull'), { state: 'old' }],
        [read('/user'), { id: 1 }],
        [write({ query: 'mutation', body: 'publish' }), { saved: true }],
        [read('/pull'), { state: 'new' }],
      ),
    )
    // Act and Assert
    expect(replay.respond(read('/user')).body).toEqual({ id: 1 })
    expect(replay.respond(read('/pull')).body).toEqual({ state: 'old' })
    expect(replay.respond(read('/pull')).body).toEqual({ state: 'old' })
    replay.respond(write({ query: 'mutation', body: 'publish' }))
    expect(replay.respond(read('/pull')).body).toEqual({ state: 'new' })
    expect(replay.respond(read('/user')).body).toEqual({ id: 1 })
    expect(() => replay.respond(read('/unknown'))).toThrow('Unrecorded')
  })

  it('should_tolerate_metadata_reads_interleaved_with_parallel_mutations', () => {
    // Arrange
    const first = write({ query: 'mutation', path: 'first' }),
      second = write({ query: 'mutation', path: 'second' })
    const replay = new CassetteReplay(
      cassette(
        [read('/id'), { id: 'PR_1' }],
        [read('/viewed'), { paths: [] }],
        [read('/id'), { id: 'PR_1' }],
        [first, { saved: true }],
        [second, { saved: true }],
        [read('/viewed'), { paths: ['first', 'second'] }],
      ),
    )
    // Act and Assert
    replay.respond(read('/id'))
    replay.respond(first)
    expect(replay.respond(read('/id')).body).toEqual({ id: 'PR_1' })
    expect(replay.respond(read('/viewed')).body).toEqual({ paths: [] })
    replay.respond(second)
    expect(replay.respond(read('/viewed')).body).toEqual({ paths: ['first', 'second'] })
    expect(() => replay.verify()).not.toThrow()
  })

  it('should_replay_eventual_consistency_then_repeat_the_last_recorded_observation', () => {
    // Arrange
    const replay = new CassetteReplay(
      cassette([read('/pull'), { head: 'old' }], [read('/pull'), { head: 'new' }]),
    )
    // Act and Assert
    expect(replay.respond(read('/pull')).body).toEqual({ head: 'old' })
    expect(replay.respond(read('/pull')).body).toEqual({ head: 'new' })
    expect(replay.respond(read('/pull')).body).toEqual({ head: 'new' })
  })

  it('should_reject_unknown_paths_accounts_payloads_and_duplicate_writes', () => {
    // Arrange
    const request = write({ query: 'mutation', input: { thread: 'THREAD_1', body: 'Exact text' } })
    const replay = new CassetteReplay(cassette([request, { saved: true }]))
    // Act and Assert
    expect(() => replay.respond({ ...request, path: '/elsewhere' })).toThrow('Unrecorded')
    expect(() => replay.respond({ ...request, actor: 'secondary' })).toThrow('Unrecorded')
    expect(() =>
      replay.respond(
        write({ query: 'mutation', input: { thread: 'THREAD_2', body: 'Exact text' } }),
      ),
    ).toThrow('Unrecorded')
    expect(() => replay.respond(write({ ...request.body, unexpected: true }))).toThrow('Unrecorded')
    replay.respond(request)
    expect(() => replay.respond(request)).toThrow('Unrecorded')
  })

  it('should_fail_if_a_recorded_mutation_was_omitted', () => {
    // Arrange
    const replay = new CassetteReplay(cassette([write({ query: 'mutation' }), { saved: true }]))
    // Act and Assert
    expect(() => replay.verify()).toThrow('omitted 1 recorded GitHub writes')
  })
})

describe('When refreshing a recording', () => {
  it('should_replace_only_selected_journeys_and_preserve_concurrent_sibling_updates', async () => {
    // Arrange
    const directory = await mkdtemp(join(tmpdir(), 'lgtm-recording-'))
    const file = join(directory, 'spec.json')
    const untouched = cassette([read('/original'), { retained: true }])
    await writeFile(file, JSON.stringify({ untouched, selected: cassette() }))
    const live = await LiveGithubDriver.start({ GH_TOKEN: 'unit-test-private-credential' })
    const selected = await RecordedGithubDriver.start(file, 'selected', cassette().fixture, live)
    const added = await RecordedGithubDriver.start(file, 'added', cassette().fixture, live)
    selected.cassette.responses.push({ status: 200, headers: {}, body: { refreshed: true } })
    try {
      // Act
      await Promise.all([selected.finish(true), added.finish(true)])
      const saved = JSON.parse(await readFile(file, 'utf8'))

      // Assert
      expect(Object.keys(saved)).toEqual(['added', 'selected', 'untouched'])
      expect(saved.untouched).toEqual(untouched)
      expect(saved.selected).toEqual(selected.cassette)
      expect(saved.added).toEqual(added.cassette)
      const replay = await RecordedGithubDriver.start(file, 'untouched')
      try {
        expect(await replay.api('original', 'GET', undefined, replay.env)).toEqual({
          retained: true,
        })
        await replay.finish(true)
      } finally {
        await replay.dispose()
      }
      await expect(RecordedGithubDriver.start(file, 'missing')).rejects.toThrow('Missing recording')
    } finally {
      await selected.dispose()
      await added.dispose()
      await rm(directory, { recursive: true, force: true })
    }
  })

  it('should_preserve_an_invalid_spec_file_instead_of_overwriting_its_journeys', async () => {
    // Arrange
    const directory = await mkdtemp(join(tmpdir(), 'lgtm-recording-'))
    const file = join(directory, 'spec.json')
    const original = '{"untouched":'
    await writeFile(file, original)
    const live = await LiveGithubDriver.start({ GH_TOKEN: 'unit-test-private-credential' })
    const driver = await RecordedGithubDriver.start(file, 'selected', cassette().fixture, live)
    try {
      // Act and Assert
      await expect(driver.finish(true)).rejects.toThrow('Missing or invalid recording')
      expect(await readFile(file, 'utf8')).toBe(original)
    } finally {
      await driver.dispose()
      await rm(directory, { recursive: true, force: true })
    }
  })

  it('should_preserve_the_existing_JSON_when_a_recorded_test_fails', async () => {
    // Arrange
    const directory = await mkdtemp(join(tmpdir(), 'lgtm-recording-'))
    const file = join(directory, 'journey.json')
    const original = JSON.stringify({ selected: cassette(), untouched: cassette() })
    await writeFile(file, original)
    const live = await LiveGithubDriver.start({ GH_TOKEN: 'unit-test-private-credential' })
    const driver = await RecordedGithubDriver.start(file, 'selected', cassette().fixture, live)
    try {
      // Act
      await driver.finish(false)
      // Assert
      expect(await readFile(file, 'utf8')).toBe(original)
    } finally {
      await driver.dispose()
      await rm(directory, { recursive: true, force: true })
    }
  })

  it('should_ignore_an_incomplete_request_cancelled_before_upstream_dispatch', async () => {
    // Arrange
    const directory = await mkdtemp(join(tmpdir(), 'lgtm-recording-'))
    const file = join(directory, 'journey.json')
    const live = await LiveGithubDriver.start({ GH_TOKEN: 'unit-test-private-credential' })
    const driver = await RecordedGithubDriver.start(file, 'selected', cassette().fixture, live)
    const pending = request(`${driver.env.LGTM_GITHUB_TEST_API_URL}/graphql`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: 'token lgtm-fixture-primary' },
    })
    pending.on('error', () => {})
    try {
      // Act
      pending.write('{"query":')
      await delay(20)
      pending.destroy()
      await delay(20)
      await driver.finish(true)
      // Assert
      expect(JSON.parse(await readFile(file, 'utf8')).selected.exchanges).toEqual([])
    } finally {
      pending.destroy()
      await driver.dispose()
      await rm(directory, { recursive: true, force: true })
    }
  })

  it('should_reject_credentials_without_replacing_the_existing_JSON', async () => {
    // Arrange
    const directory = await mkdtemp(join(tmpdir(), 'lgtm-recording-'))
    const file = join(directory, 'journey.json')
    const original = JSON.stringify({ selected: cassette(), untouched: cassette() })
    await writeFile(file, original)
    const secret = 'unit-test-private-credential'
    const live = await LiveGithubDriver.start({ GH_TOKEN: secret })
    const driver = await RecordedGithubDriver.start(file, 'selected', cassette().fixture, live)
    driver.cassette.responses.push({ status: 200, headers: {}, body: { sensitive: secret } })
    try {
      // Act and Assert
      await expect(driver.finish(true)).rejects.toThrow('Refusing to save a GitHub credential')
      expect(await readFile(file, 'utf8')).toBe(original)
    } finally {
      await driver.dispose()
      await rm(directory, { recursive: true, force: true })
    }
  })
})
