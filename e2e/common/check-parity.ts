import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const entries = JSON.parse(await readFile(join(root, 'parity.json'), 'utf8')) as {
  legacy: string
  behavior: string
  replacement: string
  steps: string[]
}[]
const key = (file: string, name: string) => `${file}\n${name}`
const mapped = new Set(entries.map((entry) => key(entry.legacy, entry.behavior)))
assert.equal(mapped.size, entries.length, 'Each legacy behavior has one coverage-map entry')

// Legacy source is archived outside the checkout. The frozen map records its
// behavior inventory; this check validates references in the active suites only.
for (const entry of entries) {
  const replacement = await readFile(join(root, entry.replacement), 'utf8')
  assert(entry.steps.length > 0, `Missing journey steps: ${entry.behavior}`)
  for (const step of entry.steps)
    assert(replacement.includes(step), `Replacement step missing: ${entry.replacement} · ${step}`)
}
console.info(
  `Historical coverage map checked: ${entries.length} behaviors reference active journey steps. Run the suites to verify their assertions.`,
)
