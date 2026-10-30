const suites = new Set(['backend', 'cli', 'nvim', 'all'])
const suite = process.argv[2]

if (!suite || !suites.has(suite)) {
  console.error('Use e2e:record -- backend|cli|nvim|all [Vitest filters]')
} else {
  console.error(
    `No recordable GitHub journeys are active for ${suite}; the recorder and existing cassettes are retained for replacement journeys.`,
  )
}
process.exitCode = 2
