import { setTimeout as delay } from 'node:timers/promises'
import { randomUUID } from 'node:crypto'
import { command, type Json, type Runtime } from './runtime.js'

const githubRepository = process.env.LGTM_E2E_GITHUB_REPOSITORY ?? 'skapoor8/lgtm-e2e'

/** Only run-owned refs, content and PRs can be mutated. No repository provisioning API exists here. */
export class GithubFixture {
  readonly run = `lgtm-e2e-${randomUUID()}`
  private readonly baseBranch = `${this.run}-base`
  private readonly headBranch = `${this.run}-head`
  readonly path = `${this.run}.txt`
  private readonly sourceTail =
    Array.from({ length: 40 }, (_, index) => `stable context ${index + 1}`).join('\n') + '\n'
  private readonly baseContent = 'first\nsecond\n' + this.sourceTail
  readonly headContent = 'first\nchanged\n' + this.sourceTail
  repositoryUrl = ''
  private base = ''
  head = ''
  number?: number
  private refs: string[] = []
  private additionalPullRequests: number[] = []
  private initialHead = ''
  private credentials: NodeJS.ProcessEnv[] = []
  private constructor(
    readonly repository: string,
    readonly env: NodeJS.ProcessEnv,
  ) {}

  /** Restore only this run's PR and refs, so cases do not inherit previous feedback. */
  async reset() {
    for (const env of [this.env, ...this.credentials]) {
      const reviews = await this.api<Json[]>(
        `repos/${this.repository}/pulls/${this.number}/reviews?per_page=100`,
        'GET',
        undefined,
        env,
      )
      for (const review of reviews)
        if (review.state === 'PENDING')
          await this.api(
            `repos/${this.repository}/pulls/${this.number}/reviews/${review.id}`,
            'DELETE',
            undefined,
            env,
          )
    }
    const comments = await this.api<Json[]>(
      `repos/${this.repository}/pulls/${this.number}/comments?per_page=100`,
    )
    for (const comment of [...comments].reverse())
      await this.api(`repos/${this.repository}/pulls/comments/${comment.id}`, 'DELETE')
    const discussion = await this.api<Json[]>(
      `repos/${this.repository}/issues/${this.number}/comments?per_page=100`,
    )
    for (const comment of discussion)
      await this.api(`repos/${this.repository}/issues/comments/${comment.id}`, 'DELETE')
    await this.api(`repos/${this.repository}/git/refs/heads/${this.headBranch}`, 'PATCH', {
      sha: this.initialHead,
      force: true,
    })
    await this.api(`repos/${this.repository}/pulls/${this.number}`, 'PATCH', { state: 'open' })
    this.head = this.initialHead
    await this.waitForHead(this.head)
  }

  /** A second PR for the same branch exercises real picker navigation. */
  async createBranchComparison() {
    const branch = `${this.run}-alternate-base`
    await this.api(`repos/${this.repository}/git/refs`, 'POST', {
      ref: `refs/heads/${branch}`,
      sha: this.base,
    })
    this.refs.push(branch)
    const pull = await this.api(`repos/${this.repository}/pulls`, 'POST', {
      title: `Alternate comparison · ${this.run}`,
      head: this.headBranch,
      base: branch,
    })
    this.additionalPullRequests.push(pull.number)
    return pull
  }

  /** Commit real additions, edits, renames or deletions atomically on the owned head. */
  async changeFiles(files: Record<string, string | null>) {
    const parent = await this.api(`repos/${this.repository}/git/commits/${this.head}`)
    const tree = await this.api(`repos/${this.repository}/git/trees`, 'POST', {
      base_tree: parent.tree.sha,
      tree: Object.entries(files).map(([path, content]) => ({
        path,
        mode: '100644',
        type: 'blob',
        ...(content === null ? { sha: null } : { content }),
      })),
    })
    const commit = await this.api(`repos/${this.repository}/git/commits`, 'POST', {
      message: 'E2E: update review comparison',
      tree: tree.sha,
      parents: [this.head],
    })
    await this.api(`repos/${this.repository}/git/refs/heads/${this.headBranch}`, 'PATCH', {
      sha: commit.sha,
    })
    this.head = commit.sha
    await this.waitForHead(this.head)
  }

  /** A ref write can finish before GitHub updates its PR metadata. */
  private async waitForHead(head: string) {
    const deadline = Date.now() + 20_000
    while (Date.now() < deadline) {
      const pull = await this.api(`repos/${this.repository}/pulls/${this.number}`)
      if (pull.head.sha === head) return
      await delay(1000)
    }
    throw new Error(`GitHub has not updated the head of fixture PR ${this.number}`)
  }

  /** GitHub search is indexed asynchronously after creating a PR. */
  async waitForSearch(number = this.number) {
    const query = encodeURIComponent(
      `repo:${this.repository} is:pr is:open head:${this.headBranch}`,
    )
    const deadline = Date.now() + 40_000
    while (Date.now() < deadline) {
      const result = await this.api(`search/issues?q=${query}`)
      if (result.items?.some((item: Json) => item.number === number)) return
      await delay(1000)
    }
    throw new Error(`GitHub has not indexed fixture PR ${number}`)
  }

  async checkout(runtime: Runtime) {
    await runtime.git('init', '-q', '-b', this.headBranch)
    await runtime.git('remote', 'add', 'origin', `${this.repositoryUrl}.git`)
    await runtime.git(
      '-c',
      'credential.helper=!gh auth git-credential',
      'fetch',
      '--quiet',
      'origin',
      `+refs/heads/${this.baseBranch}:refs/remotes/origin/${this.baseBranch}`,
      `+refs/heads/${this.headBranch}:refs/remotes/origin/${this.headBranch}`,
    )
    await runtime.git('checkout', '-q', '-B', this.headBranch, `origin/${this.headBranch}`)
  }

  async api<T = Json>(route: string, method = 'GET', body?: Json, env = this.env): Promise<T> {
    if (
      env.GH_TOKEN !== this.env.GH_TOKEN &&
      !this.credentials.some((known) => known.GH_TOKEN === env.GH_TOKEN)
    )
      this.credentials.push(env)
    if (method !== 'GET') {
      const readQuery = route === 'graphql' && /^\s*query\b/.test(body?.query ?? '')
      const fixtureResource =
        this.repositoryUrl &&
        ['git/', 'contents/', 'pulls', 'issues/'].some((resource) =>
          route.startsWith(`repos/${this.repository}/${resource}`),
        )
      if (!readQuery && !fixtureResource)
        throw new Error(
          'Tests can mutate fixture resources only; repository provisioning or changes are forbidden',
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
      {
        input: body ? JSON.stringify(body) : undefined,
        timeout: 20_000,
        env: { ...process.env, ...env },
      },
    )
    if (result.code) throw new Error(`GitHub ${method} ${route}: ${result.stderr}`)
    return (result.stdout ? JSON.parse(result.stdout) : {}) as T
  }

  async dispose() {
    const errors: unknown[] = []
    for (const number of [...(this.number ? [this.number] : []), ...this.additionalPullRequests])
      try {
        await this.api(`repos/${this.repository}/pulls/${number}`, 'PATCH', {
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
    if (errors.length)
      throw new AggregateError(
        errors,
        `Inspect run ${this.run} in ${this.repository}; fixture cleanup failed`,
      )
  }

  static async start(repository = githubRepository, env?: NodeJS.ProcessEnv) {
    if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository))
      throw new Error('LGTM_E2E_GITHUB_REPOSITORY must be owner/repository')
    const fixture = new GithubFixture(repository, env ?? (await githubCredentials()))
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
        ref: `refs/heads/${fixture.baseBranch}`,
        sha: defaultRef.object.sha,
      })
      fixture.refs.push(fixture.baseBranch)
      const initial = await fixture.api(`repos/${repository}/contents/${fixture.path}`, 'PUT', {
        branch: fixture.baseBranch,
        message: 'E2E: establish isolated review base',
        content: Buffer.from(fixture.baseContent).toString('base64'),
      })
      fixture.base = initial.commit.sha
      await fixture.api(`repos/${repository}/git/refs`, 'POST', {
        ref: `refs/heads/${fixture.headBranch}`,
        sha: fixture.base,
      })
      fixture.refs.push(fixture.headBranch)
      const updated = await fixture.api(`repos/${repository}/contents/${fixture.path}`, 'PUT', {
        branch: fixture.headBranch,
        message: 'E2E: establish isolated review head',
        sha: initial.content.sha,
        content: Buffer.from(fixture.headContent).toString('base64'),
      })
      fixture.head = updated.commit.sha
      fixture.initialHead = fixture.head
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
        throw new AggregateError([error, cleanup], 'GitHub fixture setup and cleanup failed', {
          cause: cleanup,
        })
      }
      throw error
    }
  }
}

/** Read an existing login or CI token without logging or writing it to disk. */
export async function githubCredentials(account?: string) {
  if (account && process.env.LGTM_E2E_GITHUB_SECONDARY_TOKEN)
    return { GH_TOKEN: process.env.LGTM_E2E_GITHUB_SECONDARY_TOKEN }
  if (!account && (process.env.GH_TOKEN || process.env.GITHUB_TOKEN))
    return { GH_TOKEN: process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN! }
  const selected = account ?? process.env.LGTM_E2E_GITHUB_PRIMARY_USER ?? 'skapoor8'
  const env = { ...process.env }
  delete env.GH_TOKEN
  delete env.GITHUB_TOKEN
  const result = await command(
    'gh',
    ['auth', 'token', '--hostname', 'github.com', '--user', selected],
    { env },
  )
  if (result.code || !result.stdout.trim())
    throw new Error(
      `GitHub credentials unavailable for ${selected}. Run gh auth login or set the matching E2E token.`,
    )
  return { GH_TOKEN: result.stdout.trim() }
}
