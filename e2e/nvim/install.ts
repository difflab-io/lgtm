import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { existsSync } from 'node:fs'
import { readFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'

const exec = promisify(execFile),
  suite = import.meta.dirname
const pins = JSON.parse(await readFile(join(suite, 'dependencies.lock.json'), 'utf8')) as Record<
  string,
  { repository: string; commit: string }
>
await mkdir(join(suite, '.cache'), { recursive: true })
for (const [name, pin] of Object.entries(pins)) {
  const directory = join(suite, '.cache', name)
  if (!existsSync(directory))
    await exec('git', ['clone', '--no-checkout', pin.repository, directory])
  const actual = await exec('git', ['-C', directory, 'rev-parse', 'HEAD'])
    .then((r) => r.stdout.trim())
    .catch(() => '')
  if (actual !== pin.commit) {
    await exec('git', ['-C', directory, 'fetch', 'origin', pin.commit])
    await exec('git', ['-C', directory, 'checkout', '--detach', pin.commit])
  }
  console.info(`${name}: ${pin.commit}`)
}
