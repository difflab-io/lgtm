import { randomUUID } from 'node:crypto'
import { expect } from 'vitest'
import { command, type Json } from './runtime.js'

export const githubRepository = process.env.LGTM_E2E_GITHUB_REPOSITORY ?? 'skapoor8/lgtm-e2e'

/** Only run-owned refs, content and PRs can be mutated. No repository provisioning API exists here. */
export class LiveGithub {
  readonly run = `lgtm-e2e-${randomUUID()}`
  readonly baseBranch = `${this.run}-base`
  readonly headBranch = `${this.run}-head`
  readonly path = `${this.run}.txt`
  repositoryUrl = ''
  base = ''
  head = ''
  blob = ''
  number?: number
  private lock = false
  private refs: string[] = []
  private constructor(readonly repository: string) {}

  async api(route: string, method = 'GET', body?: Json): Promise<Json> {
    if (method !== 'GET') {
      const readQuery = route === 'graphql' && /^\s*query\b/.test(body?.query ?? '')
      const fixtureResource = this.repositoryUrl && route.startsWith(`repos/${this.repository}/`)
      if (!readQuery && !fixtureResource)
        throw new Error(
          'Live tests can mutate fixture resources only; repository provisioning or changes are forbidden',
        )
    }
    const result = await command(
      'gh',
      [
        'api',
        '--hostname',
        'github.com',
        '--method',
        method,
        route,
        ...(body ? ['--input', '-'] : []),
      ],
      { input: body ? JSON.stringify(body) : undefined, timeout: 20_000 },
    )
    if (result.code) throw new Error(`GitHub ${method} ${route}: ${result.stderr}`)
    return result.stdout ? JSON.parse(result.stdout) : {}
  }

  async push(text: string) {
    const result = await this.api(`repos/${this.repository}/contents/${this.path}`, 'PUT', {
      branch: this.headBranch,
      message: 'E2E: change selected source',
      sha: this.blob,
      content: Buffer.from(text).toString('base64'),
    })
    this.head = result.commit.sha
    this.blob = result.content.sha
    return result
  }

  async threads(): Promise<Json[]> {
    const [owner, name] = this.repository.split('/')
    const response = await this.api('graphql', 'POST', {
      query:
        'query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{path line originalLine subjectType comments(first:100){nodes{databaseId body}}}}}}}',
      variables: { owner, name, number: this.number },
    })
    expect(response.errors).toBeUndefined()
    return response.data.repository.pullRequest.reviewThreads.nodes
  }

  async dispose() {
    const errors: unknown[] = []
    if (this.number)
      try {
        await this.api(`repos/${this.repository}/pulls/${this.number}`, 'PATCH', {
          state: 'closed',
        })
      } catch (error) {
        errors.push(error)
      }
    for (const branch of [...this.refs].reverse())
      try {
        await this.api(`repos/${this.repository}/git/refs/heads/${branch}`, 'DELETE')
      } catch (error) {
        errors.push(error)
      }
    if (this.lock)
      try {
        await this.api(`repos/${this.repository}/git/refs/heads/lgtm-e2e-lock`, 'DELETE')
      } catch (error) {
        errors.push(error)
      }
    if (errors.length)
      throw new AggregateError(
        errors,
        `Inspect run ${this.run} in ${this.repository}; fixture cleanup failed`,
      )
  }

  static async start(repository = githubRepository) {
    if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository))
      throw new Error('LGTM_E2E_GITHUB_REPOSITORY must be owner/repository')
    const fixture = new LiveGithub(repository)
    // Missing, public or archived repositories fail before any writes. Never provision a replacement.
    const metadata = await fixture.api(`repos/${repository}`)
    if (metadata.private !== true || metadata.visibility !== 'private' || metadata.archived)
      throw new Error(`Configure an existing, active PRIVATE fixture repository: ${repository}`)
    fixture.repositoryUrl = metadata.html_url
    const defaultRef = await fixture.api(
      `repos/${repository}/git/ref/heads/${encodeURIComponent(metadata.default_branch)}`,
    )
    try {
      await fixture.api(`repos/${repository}/git/refs`, 'POST', {
        ref: 'refs/heads/lgtm-e2e-lock',
        sha: defaultRef.object.sha,
      })
      fixture.lock = true
      await fixture.api(`repos/${repository}/git/refs`, 'POST', {
        ref: `refs/heads/${fixture.baseBranch}`,
        sha: defaultRef.object.sha,
      })
      fixture.refs.push(fixture.baseBranch)
      const initial = await fixture.api(`repos/${repository}/contents/${fixture.path}`, 'PUT', {
        branch: fixture.baseBranch,
        message: 'E2E: establish isolated review base',
        content: Buffer.from('first\nsecond\n').toString('base64'),
      })
      fixture.base = initial.commit.sha
      fixture.blob = initial.content.sha
      await fixture.api(`repos/${repository}/git/refs`, 'POST', {
        ref: `refs/heads/${fixture.headBranch}`,
        sha: fixture.base,
      })
      fixture.refs.push(fixture.headBranch)
      await fixture.push('first\nchanged\n')
      const pr = await fixture.api(`repos/${repository}/pulls`, 'POST', {
        title: `LGTM E2E · ${fixture.run}`,
        head: fixture.headBranch,
        base: fixture.baseBranch,
        body: 'Isolated review lifecycle test in the permanent private fixture repository. Closed after this run.',
      })
      fixture.number = pr.number
      console.info(`Persistent private fixture: ${pr.html_url}`)
      return fixture
    } catch (error) {
      try {
        await fixture.dispose()
      } catch (cleanup) {
        throw new AggregateError([error, cleanup], 'Live fixture setup and cleanup failed', {
          cause: cleanup,
        })
      }
      throw error
    }
  }
}
