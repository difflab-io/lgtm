import type { Json } from './test-utils.js'
import type { GithubFixtureState } from './github-driver.js'

export type Exchange = {
  request: { actor: string; method: string; path: string; body: Json | null }
  response: { status: number; headers: Record<string, string>; body: Json | Json[] | null }
}
export type Cassette = {
  version: 1
  fixture: GithubFixtureState
  gitBundle?: string
  responses: Exchange['response'][]
  exchanges: { request: Exchange['request']; response: number }[]
}
const uuid = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi
export const isRead = (request: Exchange['request']) =>
  request.method === 'GET' ||
  (request.path === '/graphql' && /^\s*query\b/.test(request.body?.query ?? ''))

/** Bind runtime-generated UUIDs consistently; every other request field must match. */
function match(expected: unknown, actual: unknown, bindings: Map<string, string>): boolean {
  if (typeof expected === 'string' && typeof actual === 'string') {
    const originals = expected.match(uuid) ?? [],
      values = actual.match(uuid) ?? []
    if (
      expected.replace(uuid, '<uuid>') !== actual.replace(uuid, '<uuid>') ||
      originals.length !== values.length
    )
      return false
    return originals.every((original, index) => {
      if (bindings.has(original)) return bindings.get(original) === values[index]
      if ([...bindings.values()].includes(values[index])) return false
      bindings.set(original, values[index])
      return true
    })
  }
  if (
    expected === null ||
    actual === null ||
    typeof expected !== 'object' ||
    typeof actual !== 'object'
  )
    return expected === actual
  if (Array.isArray(expected) !== Array.isArray(actual)) return false
  const left = expected as Json,
    right = actual as Json
  return (
    JSON.stringify(Object.keys(left).sort()) === JSON.stringify(Object.keys(right).sort()) &&
    Object.keys(left).every((key) => match(left[key], right[key], bindings))
  )
}

/** Writes are ordered barriers; reads can reuse recorded past observations, never future ones. */
export class CassetteReplay {
  private readonly phases: { reads: Exchange[]; write?: Exchange; used: Set<number> }[] = [
    { reads: [], used: new Set() },
  ]
  private phase = 0
  private bindings = new Map<string, string>()
  constructor(cassette: Cassette) {
    for (const fixed of cassette.fixture.run.match(uuid) ?? []) this.bindings.set(fixed, fixed)
    for (const saved of cassette.exchanges) {
      const exchange: Exchange = {
        request: saved.request,
        response: cassette.responses[saved.response],
      }
      if (!exchange.response) throw new Error('Invalid GitHub recording response reference')
      const phase = this.phases.at(-1)!
      if (isRead(exchange.request)) phase.reads.push(exchange)
      else {
        phase.write = exchange
        this.phases.push({ reads: [], used: new Set() })
      }
    }
  }

  respond(request: Exchange['request']) {
    const phase = this.phases[this.phase]
    const candidates = isRead(request)
      ? [
          ...phase.reads
            .map((exchange, index) => ({ exchange, index }))
            .filter(({ index }) => !phase.used.has(index)),
          ...phase.reads.map((exchange, index) => ({ exchange, index })).reverse(),
          ...this.phases
            .slice(0, this.phase)
            .reverse()
            .flatMap((past) =>
              past.reads.toReversed().map((exchange) => ({ exchange, index: -2 })),
            ),
        ]
      : phase.write
        ? [{ exchange: phase.write, index: -1 }]
        : []
    for (const { exchange, index } of candidates) {
      const bindings = new Map(this.bindings)
      if (!match(exchange.request, request, bindings)) continue
      this.bindings = bindings
      if (index === -1) this.phase++
      else if (index >= 0) phase.used.add(index)
      return JSON.parse(
        JSON.stringify(exchange.response).replace(uuid, (value) => bindings.get(value) ?? value),
      ) as Exchange['response']
    }
    throw new Error(
      `Unrecorded GitHub request in phase ${this.phase}: ${JSON.stringify(request)}\nNext write: ${JSON.stringify(phase.write?.request ?? null)}. Refresh this test with e2e:record.`,
    )
  }

  verify() {
    if (this.phase !== this.phases.length - 1)
      throw new Error(
        `Replay omitted ${this.phases.length - 1 - this.phase} recorded GitHub writes`,
      )
  }
}
