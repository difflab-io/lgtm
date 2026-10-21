import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import ts from 'typescript'

const root = resolve(import.meta.dirname, '..')
const entries = JSON.parse(await readFile(join(root, 'parity.json'), 'utf8')) as {
  legacy: string
  behavior: string
  replacements: { file: string; scenarios: string[] }[]
}[]
const key = (file: string, name: string) => `${file}\n${name}`
const mapped = new Set(entries.map((entry) => key(entry.legacy, entry.behavior)))
assert.equal(mapped.size, entries.length, 'Each legacy behavior has one coverage-map entry')

// Legacy source is archived outside the checkout. The frozen map records its
// behavior inventory; this check validates references in the active suites only.
const files = new Set(entries.flatMap((entry) => entry.replacements.map(({ file }) => file)))
const declared = new Map<string, Set<string>>()
for (const file of files) {
  const contents = await readFile(join(root, file), 'utf8')
  if (file.endsWith('.rs')) {
    declared.set(
      file,
      new Set([...contents.matchAll(/async fn (should_\w+)\(/g)].map((match) => match[1])),
    )
    continue
  }
  const source = ts.createSourceFile(file, contents, ts.ScriptTarget.Latest, true)
  const names = new Set<string>()
  function visit(node: ts.Node) {
    if (ts.isCallExpression(node)) {
      const callee = node.expression.getText(source)
      const title = node.arguments[0]
      if ((callee === 'it' || callee.startsWith('it.each(')) && title && ts.isStringLiteral(title))
        names.add(title.text)
    }
    ts.forEachChild(node, visit)
  }
  visit(source)
  declared.set(file, names)
}
for (const entry of entries) {
  assert(entry.replacements.length > 0, `Missing feature specifications: ${entry.behavior}`)
  for (const { file, scenarios } of entry.replacements) {
    assert(scenarios.length > 0, `Missing test declarations: ${entry.behavior}`)
    for (const scenario of scenarios)
      assert(
        declared.get(file)?.has(scenario),
        `Replacement scenario missing: ${file} · ${scenario}`,
      )
  }
}
console.info(
  `Historical coverage map checked: ${entries.length} behaviors reference current test declarations. Run the suites to verify their assertions.`,
)
