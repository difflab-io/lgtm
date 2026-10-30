import { setTimeout as delay } from 'node:timers/promises'
import { randomUUID } from 'node:crypto'
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { command, ROOT, type TestHarness } from './test-harness.js'
import { type GithubDriver, type GithubFixtureState, LiveGithubDriver } from './github-driver.js'
import { RecordedGithubDriver } from './github-recording.js'
import { isolatedEnv, type Json } from './test-utils.js'

const githubRepository = process.env.LGTM_E2E_GITHUB_REPOSITORY ?? 'skapoor8/lgtm-e2e'

/** Only run-owned refs, content and PRs can be mutated. No repository provisioning API exists here. */
export class GithubFixture {
  private get baseBranch() {
    return `${this.run}-base`
  }
  private get headBranch() {
    return `${this.run}-head`
  }
  get path() {
    return `${this.run}.txt`
  }
  private readonly sourceTail =
    Array.from({ length: 40 }, (_, index) => `stable context ${index + 1}`).join('\n') + '\n'
  private readonly baseContent = 'first\nsecond\n' + this.sourceTail
  readonly headContent = 'first\nchanged\n' + this.sourceTail
  repositoryUrl = ''
  private nodeId = ''
  private base = ''
  head = ''
  number?: number
  private refs: string[] = []
  private additionalPullRequests: number[] = []
  private initialHead = ''
  private credentialEnvironments: NodeJS.ProcessEnv[] = []
  private constructor(
    readonly repository: string,
    private driver: GithubDriver,
    readonly run = `lgtm-e2e-${randomUUID()}`,
  ) {}
  private liveDriver?: LiveGithubDriver
  private workspace?: string

  get env() {
    return this.driver.env
  }

  async prepareSnapshots() {
    await this.driver.prepareGit?.([this.base, this.head])
    if (this.driver.gitSource) {
      this.env.LGTM_GITHUB_TEST_GIT_URL = this.driver.gitSource
    }
  }

  credentials(account?: string) {
    return this.driver.credentials(account)
  }

  async record(file: string, journey: string) {
    this.liveDriver = this.driver as LiveGithubDriver
    const fixture: GithubFixtureState = {
      repository: this.repository,
      repositoryUrl: this.repositoryUrl,
      run: this.run,
      nodeId: this.nodeId,
      base: this.base,
      head: this.head,
      initialHead: this.initialHead,
      number: this.number!,
    }
    this.driver = await RecordedGithubDriver.start(file, journey, fixture, this.liveDriver)
    this.credentialEnvironments = []
  }

  async finish(passed: boolean) {
    if (!(this.driver instanceof RecordedGithubDriver)) return
    const driver = this.driver
    if (this.liveDriver) {
      this.driver = this.liveDriver
      this.liveDriver = undefined
    }
    await driver.finish(passed)
  }

  static async replay(file: string, journey: string) {
    const driver = await RecordedGithubDriver.start(file, journey)
    const state = driver.cassette.fixture
    const fixture = new GithubFixture(state.repository, driver, state.run)
    Object.assign(fixture, {
      repositoryUrl: state.repositoryUrl,
      nodeId: state.nodeId,
      base: state.base,
      head: state.head,
      initialHead: state.initialHead,
      number: state.number,
    })
    return fixture
  }

  /** Restore only this run's PR and refs, so cases do not inherit previous feedback. */
  async reset(pull: Json) {
    for (const env of [this.env, ...this.credentialEnvironments]) {
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
    const changedHead = pull.head.sha !== this.initialHead
    if (changedHead)
      await this.api(`repos/${this.repository}/git/refs/heads/${this.headBranch}`, 'PATCH', {
        sha: this.initialHead,
        force: true,
      })
    if (pull.state !== 'open')
      await this.api(`repos/${this.repository}/pulls/${this.number}`, 'PATCH', { state: 'open' })
    this.head = this.initialHead
    if (changedHead) await this.waitForHead(this.head)
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
    await this.driver.prepareGit?.([this.head])
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

  async checkout(runtime: TestHarness) {
    await runtime.git('init', '-q', '-b', this.headBranch)
    await runtime.git('remote', 'add', 'origin', `${this.repositoryUrl}.git`)
    await this.fetch(
      runtime,
      `+refs/heads/${this.baseBranch}:refs/remotes/origin/${this.baseBranch}`,
      `+refs/heads/${this.headBranch}:refs/remotes/origin/${this.headBranch}`,
    )
    await runtime.git('checkout', '-q', '-B', this.headBranch, `origin/${this.headBranch}`)
  }

  /** Fetch real Git objects while leaving origin usable for forge detection. */
  async fetch(runtime: TestHarness, ...refs: string[]) {
    await this.driver.prepareGit?.(refs)
    await runtime.git(
      '-c',
      'credential.helper=!gh auth git-credential',
      'fetch',
      '--quiet',
      this.driver.gitSource ?? 'origin',
      ...refs,
    )
  }

  async api<T = Json>(route: string, method = 'GET', body?: Json, env = this.env): Promise<T> {
    if (
      env.GH_TOKEN !== this.env.GH_TOKEN &&
      !this.credentialEnvironments.some((known) => known.GH_TOKEN === env.GH_TOKEN)
    )
      this.credentialEnvironments.push(env)
    if (method !== 'GET') {
      const readQuery = route === 'graphql' && /^\s*query\b/.test(body?.query ?? '')
      const fixtureResource =
        this.repositoryUrl &&
        ['git/', 'contents/', 'pulls', 'issues/'].some((resource) =>
          route.startsWith(`repos/${this.repository}/${resource}`),
        )
      const unmarkOwnedFile =
        route === 'graphql' &&
        this.nodeId !== '' &&
        body?.query ===
          'mutation($input:UnmarkFileAsViewedInput!){unmarkFileAsViewed(input:$input){clientMutationId}}' &&
        body?.variables?.input?.pullRequestId === this.nodeId
      if (!readQuery && !fixtureResource && !unmarkOwnedFile)
        throw new Error(
          'Tests can mutate fixture resources only; repository provisioning or changes are forbidden',
        )
    }
    return this.driver.api<T>(route, method, body, env)
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
    if (this.refs.length)
      try {
        await this.git(
          'push',
          '--quiet',
          'origin',
          ...this.refs.map((branch) => `:refs/heads/${branch}`),
        )
      } catch (error) {
        errors.push(error)
      }
    if (this.workspace) await rm(this.workspace, { recursive: true, force: true })
    await this.driver.dispose()
    if (errors.length)
      throw new AggregateError(
        errors,
        `Inspect run ${this.run} in ${this.repository}; fixture cleanup failed`,
      )
  }

  private async git(...args: string[]) {
    const result = await command(
      'git',
      ['-c', 'credential.helper=!gh auth git-credential', ...args],
      {
        cwd: this.workspace,
        env: {
          ...isolatedEnv(this.workspace!),
          ...this.env,
        },
        timeout: 30_000,
      },
    )
    if (result.code) throw new Error(`Fixture git ${args[0]}: ${result.stderr}`)
    return result.stdout.trim()
  }

  static async start(repository = githubRepository, env?: NodeJS.ProcessEnv) {
    if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository))
      throw new Error('LGTM_E2E_GITHUB_REPOSITORY must be owner/repository')
    const driver = await LiveGithubDriver.start(env)
    const fixture = new GithubFixture(repository, driver)
    try {
      // Missing, public or archived repositories fail before any writes. Never provision a replacement.
      const metadata = await fixture.api(`repos/${repository}`)
      if (metadata.private !== true || metadata.visibility !== 'private' || metadata.archived)
        throw new Error(`Configure an existing, active PRIVATE fixture repository: ${repository}`)
      fixture.repositoryUrl = metadata.html_url
      const parent = join(ROOT, 'target/e2e/github')
      await mkdir(parent, { recursive: true })
      fixture.workspace = await mkdtemp(join(parent, 'source-'))
      await fixture.git('init', '-q', '-b', fixture.baseBranch)
      await fixture.git('remote', 'add', 'origin', `${fixture.repositoryUrl}.git`)
      await fixture.git('fetch', '--quiet', 'origin', `refs/heads/${metadata.default_branch}`)
      await fixture.git('checkout', '-q', '-B', fixture.baseBranch, 'FETCH_HEAD')
      for (const [branch, content] of [
        [fixture.baseBranch, fixture.baseContent],
        [fixture.headBranch, fixture.headContent],
      ]) {
        await fixture.git('checkout', '-q', '-B', branch)
        await writeFile(join(fixture.workspace, fixture.path), content)
        await fixture.git('add', '--', fixture.path)
        await fixture.git(
          '-c',
          'user.name=E2E',
          '-c',
          'user.email=e2e@example.test',
          'commit',
          '-qm',
          'E2E: isolated review comparison',
        )
        const sha = await fixture.git('rev-parse', 'HEAD')
        if (branch === fixture.baseBranch) fixture.base = sha
        else fixture.head = sha
        await fixture.git('push', '--quiet', 'origin', `refs/heads/${branch}`)
        fixture.refs.push(branch)
      }
      fixture.initialHead = fixture.head
      const pr = await fixture.api(`repos/${repository}/pulls`, 'POST', {
        title: `LGTM E2E · ${fixture.run}`,
        head: fixture.headBranch,
        base: fixture.baseBranch,
        body: 'Isolated review lifecycle test in the permanent private fixture repository. Closed after this run.',
      })
      fixture.number = pr.number
      fixture.nodeId = pr.node_id
      console.info(`Live GitHub fixture: ${pr.html_url}`)
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
