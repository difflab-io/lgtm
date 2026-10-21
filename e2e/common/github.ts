import { createServer, type Server } from 'node:http'
import { once } from 'node:events'
import { setTimeout as delay } from 'node:timers/promises'
import type { Json } from './runtime.js'

/** An HTTP boundary for the real Octocrab client, including accepted writes with lost responses. */
export class Github {
  endpoint = ''
  mode = 'normal'
  base = 'base'
  head = 'head'
  contentReads = 0
  commentsDelay = 0
  contents: Record<string, Record<string, string>> = {}
  files: Json[] | null = null
  prReads = 0
  advanceOnRead?: number
  nextHead = ''
  reviews: Record<number, Json> = {}
  history: Record<number, Json[]> = {}
  nextReview = 10
  threads: Record<number, Json[]> = {}
  comments: Record<number, Json[]> = {}
  writes = 0
  errors: string[] = []
  requests: { method: string; path: string; query: string; search: string }[] = []
  selfPr?: number
  rejectNextEvent?: number
  private releaseMutation?: () => void
  private mutationsReady: Promise<void> = Promise.resolve()
  private server: Server

  private constructor() {
    this.server = createServer(async (request, response) => {
      let status = 500,
        result: Json | Json[]
      try {
        let raw = ''
        for await (const chunk of request) raw += chunk
        ;[status, result] = await this.request(
          request.method ?? 'GET',
          request.url ?? '/',
          raw ? JSON.parse(raw) : {},
        )
      } catch (error) {
        this.errors.push(String(error))
        result = { message: String(error) }
      }
      response.writeHead(status, { 'Content-Type': 'application/json' })
      response.end(JSON.stringify(result))
    })
  }

  control(body: Json) {
    if ('self_pr' in body) this.selfPr = body.self_pr
    if ('reject_next_event' in body) this.rejectNextEvent = body.reject_next_event
    if (body.reset_requests) this.requests = []
    if ('pause_mutations' in body) {
      if (body.pause_mutations)
        this.mutationsReady = new Promise((resolve) => {
          this.releaseMutation = resolve
        })
      else {
        this.releaseMutation?.()
        this.releaseMutation = undefined
      }
    }
    this.mode = body.mode ?? 'normal'
    this.base = body.base ?? this.base
    this.head = body.head ?? this.head
    this.commentsDelay = body.comments_delay ?? this.commentsDelay
    Object.assign(this.contents, body.contents ?? {})
    if ('files' in body) this.files = body.files
    if ('advance_on_pr_read' in body) {
      this.prReads = 0
      this.advanceOnRead = body.advance_on_pr_read
      this.nextHead = body.next_head
    }
    for (const [number, threads] of Object.entries(this.threads)) {
      for (const thread of threads) {
        if (body.shift_current_lines && thread.line != null && thread.subjectType === 'LINE') {
          thread.line += body.shift_current_lines
          thread.startLine = (thread.startLine ?? thread.originalLine) + body.shift_current_lines
        }
        if (Number(number) === body.outdated_pr) {
          thread.originalLine ??= thread.line
          thread.originalStartLine ??= thread.startLine
          thread.isOutdated = body.outdated
          thread.line = body.outdated ? null : thread.originalLine
          thread.startLine = body.outdated ? null : thread.originalStartLine
          thread.comments.nodes[0].commit = { oid: this.head }
          thread.comments.nodes[0].originalCommit ??= { oid: this.head }
        }
      }
    }
    if (body.discard_pending)
      for (const [number, review] of Object.entries(this.reviews)) {
        const n = Number(number)
        if (review.state !== 'PENDING' || (body.discard_pr ?? n) !== n) continue
        for (const thread of this.threads[n] ?? [])
          thread.comments.nodes = thread.comments.nodes.filter(
            (c: Json) => c.pullRequestReview?.databaseId !== review.id,
          )
        this.threads[n] = (this.threads[n] ?? []).filter((t) => t.comments.nodes.length)
        delete this.reviews[n]
      }
  }

  status() {
    return {
      writes: this.writes,
      reviews: Object.values(this.reviews),
      comments: this.comments,
      errors: this.errors,
      content_reads: this.contentReads,
      threads: this.threads,
      requests: this.requests,
    }
  }

  private pull(number: number) {
    if (++this.prReads === this.advanceOnRead) {
      this.head = this.nextHead
      this.advanceOnRead = undefined
    }
    return {
      number,
      title: `E2E pull ${number}`,
      user: { id: number === this.selfPr ? 1 : 2 },
      head: { sha: this.mode === 'stale' ? 'changed' : this.head },
      base: { sha: this.base },
      state: ['closed', 'merged'].includes(this.mode) ? 'closed' : 'open',
      merged: this.mode === 'merged',
    }
  }

  private comment(body: string, number: number, pending = true) {
    const id = this.writes + 100
    return {
      id: `C_${id}`,
      databaseId: id,
      body,
      updatedAt: '2026-10-18T12:00:00Z',
      author: { databaseId: 1, login: 'e2e' },
      pullRequestReview: {
        databaseId: this.reviews[number].id,
        state: pending ? 'PENDING' : this.reviews[number].state,
      },
    }
  }

  private async request(method: string, url: string, body: Json): Promise<[number, Json | Json[]]> {
    const parsed = new URL(url, 'http://fixture'),
      path = parsed.pathname
    if (!['/__control', '/__status'].includes(path))
      this.requests.push({
        method,
        path,
        query: body.query ?? '',
        search: parsed.searchParams.get('q') ?? '',
      })
    if (path === '/__control') {
      this.control(body)
      return [200, { writes: this.writes }]
    }
    if (path === '/__status') return [200, this.status()]
    if (['denied', 'limited'].includes(this.mode))
      return [this.mode === 'denied' ? 403 : 429, { message: 'Fixture permission/rate rejection' }]
    if (method === 'GET') {
      if (path === '/user') return [200, { id: 1, login: 'e2e' }]
      if (path === '/search/issues')
        return [200, { total_count: 3, items: [1, 2, 3].map((number) => ({ number })) }]
      if (path === '/repos/acme/repo/pulls') return [200, [1, 2, 3].map((n) => this.pull(n))]
      if (path.includes('/contents/')) {
        this.contentReads++
        const ref = parsed.searchParams.get('ref') ?? ''
        const text = this.contents[ref]
          ? this.contents[ref][decodeURIComponent(path.split('/contents/')[1])]
          : ref === this.base
            ? 'one\nold\nthree\n'
            : 'one\nnew\nthree\n'
        if (text == null) return [404, { message: 'File not found at requested commit' }]
        return [
          200,
          {
            size: Buffer.byteLength(text),
            encoding: 'base64',
            content: Buffer.from(text).toString('base64'),
          },
        ]
      }
      const pull = path.match(
        /^\/repos\/acme\/repo\/pulls\/(\d+)(\/files|\/reviews|\/comments|\/reviews\/\d+\/comments)?$/,
      )
      if (pull) {
        const number = Number(pull[1]),
          suffix = pull[2]
        if (suffix === '/files')
          return [200, this.files ?? [{ filename: 'review.txt', status: 'modified' }]]
        if (suffix === '/reviews')
          return [
            200,
            [
              ...(this.history[number] ?? []),
              ...(this.reviews[number] ? [this.reviews[number]] : []),
            ],
          ]
        if (suffix?.endsWith('/comments')) {
          if (suffix === '/comments' && this.reviews[number]?.state === 'PENDING') return [200, []]
          return [
            200,
            (this.threads[number] ?? []).flatMap((t) =>
              t.comments.nodes.map((c: Json) => ({
                id: c.databaseId,
                body: c.body,
                user: { id: 1, login: 'e2e' },
                updated_at: c.updatedAt,
              })),
            ),
          ]
        }
        return [200, this.pull(number)]
      }
      const issue = path.match(/^\/repos\/acme\/repo\/issues\/(\d+)\/comments$/)
      if (issue) return [200, this.comments[Number(issue[1])] ?? []]
    }
    if (['PATCH', 'DELETE'].includes(method)) {
      const match = path.match(/^\/repos\/acme\/repo\/pulls\/comments\/(\d+)$/)
      if (match) {
        for (const [number, threads] of Object.entries(this.threads))
          for (const thread of threads) {
            const comment = thread.comments.nodes.find(
              (c: Json) => c.databaseId === Number(match[1]),
            )
            if (!comment) continue
            if (this.reviews[Number(number)].state === 'PENDING')
              return [404, { message: 'Private pending comment is not available through REST' }]
            this.writes++
            if (method === 'PATCH') {
              comment.body = body.body
              return [200, comment]
            }
            thread.comments.nodes = thread.comments.nodes.filter((c: Json) => c !== comment)
            this.threads[Number(number)] = threads.filter((t) => t.comments.nodes.length)
            return [200, {}]
          }
        return [404, { message: 'Comment not found' }]
      }
    }
    if (path === '/graphql') {
      const query: string = body.query,
        variables: Json = body.variables ?? {}
      if (query.includes('reviewThreads') && this.commentsDelay)
        await delay(this.commentsDelay * 1000)
      if (query.startsWith('mutation') && this.releaseMutation) {
        let timer: ReturnType<typeof setTimeout> | undefined
        try {
          await Promise.race([
            this.mutationsReady,
            new Promise<never>((_, reject) => {
              timer = setTimeout(
                () => reject(new Error('Paused mutation was never released')),
                10_000,
              )
            }),
          ])
        } finally {
          clearTimeout(timer)
        }
      }
      if (
        query.includes('updatePullRequestReviewComment') ||
        query.includes('deletePullRequestReviewComment')
      ) {
        this.writes++
        for (const [number, threads] of Object.entries(this.threads))
          for (const thread of threads) {
            const comment = thread.comments.nodes.find((c: Json) => c.id === variables.id)
            if (!comment) continue
            if (query.includes('updatePullRequestReviewComment')) {
              comment.body = variables.body
              return [
                200,
                { data: { updatePullRequestReviewComment: { pullRequestReviewComment: comment } } },
              ]
            }
            thread.comments.nodes = thread.comments.nodes.filter((c: Json) => c !== comment)
            this.threads[Number(number)] = threads.filter((t) => t.comments.nodes.length)
            return [200, { data: { deletePullRequestReviewComment: { clientMutationId: null } } }]
          }
        return [200, { errors: [{ message: 'Review comment not found' }] }]
      }
      if (query.includes('node(id:')) {
        const thread = Object.values(this.threads)
          .flat()
          .find((t) => t.id === variables.id)
        for (const comment of thread?.comments.nodes ?? []) comment.viewerDidAuthor = true
        return [200, { data: { node: thread ?? null } }]
      }
      if (query.includes('addPullRequestReviewThreadReply')) {
        this.writes++
        const input = variables.input
        const number = Number(
          Object.keys(this.reviews).find(
            (n) => this.reviews[Number(n)].node_id === input.pullRequestReviewId,
          ),
        )
        const thread = this.threads[number].find((t) => t.id === input.pullRequestReviewThreadId)!
        const comment = this.comment(input.body, number)
        thread.comments.nodes.push(comment)
        return [200, { data: { addPullRequestReviewThreadReply: { comment } } }]
      }
      if (query.includes('resolveReviewThread')) {
        this.writes++
        const thread = Object.values(this.threads)
          .flat()
          .find((t) => t.id === variables.id)!
        thread.isResolved = !query.includes('unresolveReviewThread')
        return [
          200,
          {
            data: {
              [thread.isResolved ? 'resolveReviewThread' : 'unresolveReviewThread']: { thread },
            },
          },
        ]
      }
      if (query.includes('addPullRequestReviewThread')) {
        this.writes++
        const input = variables.input
        const number = Number(
          Object.keys(this.reviews).find(
            (n) => this.reviews[Number(n)].node_id === input.pullRequestReviewId,
          ),
        )
        const comment = this.comment(input.body, number)
        Object.assign(comment, { originalCommit: { oid: this.head }, commit: { oid: this.head } })
        const thread = {
          id: `T_${comment.databaseId}`,
          path: input.path,
          line: input.line ?? null,
          startLine: input.startLine ?? null,
          originalLine: input.line ?? null,
          originalStartLine: input.startLine ?? null,
          diffSide: input.side ?? 'RIGHT',
          subjectType: input.subjectType,
          isResolved: false,
          comments: { nodes: [comment], pageInfo: { hasNextPage: false } },
        }
        ;(this.threads[number] ??= []).push(thread)
        if (this.mode === 'draft_lost')
          return [500, { message: 'Response lost after draft accepted' }]
        return [200, { data: { addPullRequestReviewThread: { thread } } }]
      }
      if (query.includes('reviewThreads'))
        return [
          200,
          {
            data: {
              repository: {
                pullRequest: {
                  reviewThreads: {
                    nodes: this.threads[variables.number] ?? [],
                    pageInfo: { hasNextPage: false },
                  },
                },
              },
            },
          },
        ]
      throw new Error(`Unexpected GraphQL operation: ${query}`)
    }
    if (method === 'POST') {
      this.writes++
      const create = path.match(/^\/repos\/acme\/repo\/pulls\/(\d+)\/reviews$/)
      if (create) {
        const number = Number(create[1])
        if (this.reviews[number]?.state === 'PENDING')
          return [422, { message: 'A pending review already exists' }]
        if (this.reviews[number]) (this.history[number] ??= []).push(this.reviews[number])
        const id = ++this.nextReview
        const review = {
          id,
          node_id: `PRR_${id}`,
          state: 'PENDING',
          body: body.body,
          commit_id: body.commit_id,
          user: { id: 1, login: 'e2e' },
        }
        this.reviews[number] = review
        if (this.mode === 'review_lost')
          return [500, { message: 'Response lost after pending review accepted' }]
        return [200, review]
      }
      const event = path.match(/^\/repos\/acme\/repo\/pulls\/(\d+)\/reviews\/\d+\/events$/)
      if (event) {
        const number = Number(event[1]),
          review = this.reviews[number]
        if (this.rejectNextEvent) {
          const status = this.rejectNextEvent
          this.rejectNextEvent = undefined
          return [status, { message: 'Review event validation failed' }]
        }
        review.state = (
          {
            APPROVE: 'APPROVED',
            REQUEST_CHANGES: 'CHANGES_REQUESTED',
            COMMENT: 'COMMENTED',
          } as Record<string, string>
        )[body.event]
        review.body = body.body
        for (const thread of this.threads[number] ?? [])
          for (const c of thread.comments.nodes) c.pullRequestReview.state = review.state
        if (this.mode === 'lost') return [500, { message: 'Response lost after acceptance' }]
        return [200, review]
      }
      const issue = path.match(/^\/repos\/acme\/repo\/issues\/(\d+)\/comments$/)
      if (issue) {
        const comment = {
          id: this.writes + 200,
          body: body.body,
          user: { id: 1, login: 'e2e' },
          updated_at: '2026-10-18T12:00:00Z',
        }
        ;(this.comments[Number(issue[1])] ??= []).push(comment)
        return [200, comment]
      }
    }
    throw new Error(`Unexpected request ${method} ${url}`)
  }

  async close() {
    this.releaseMutation?.()
    this.server.closeAllConnections()
    await new Promise<void>((resolve) => this.server.close(() => resolve()))
  }

  static async start() {
    const fixture = new Github()
    fixture.server.listen(0, '127.0.0.1')
    await once(fixture.server, 'listening')
    const address = fixture.server.address()
    if (!address || typeof address === 'string') throw new Error('Missing fixture address')
    fixture.endpoint = `http://127.0.0.1:${address.port}`
    return fixture
  }
}
