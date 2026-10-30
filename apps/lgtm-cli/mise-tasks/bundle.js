#!/usr/bin/env bun
//MISE description="Bundle the private backend and web UI into the publishable CLI"
//MISE depends=["//apps/lgtm-backend:build", "//apps/lgtm-web:build"]

import { cp, mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'

const cli = process.cwd()
const root = resolve(cli, '../..')
const result = Bun.spawnSync(['cargo', 'build', '-p', 'lgtm-backend', '--message-format=json'], {
  cwd: root,
  stdout: 'pipe',
  stderr: 'inherit',
})
if (result.exitCode !== 0) throw new Error('Backend build failed')
const messages = result.stdout
  .toString()
  .trim()
  .split('\n')
  .map((line) => JSON.parse(line))
const generated = messages.find(
  (message) =>
    message.reason === 'build-script-executed' && message.package_id.includes('lgtm-backend'),
)?.out_dir
if (!generated) throw new Error('Backend did not report generated Protobuf output')

const stage = join(cli, '.bundle-stage')
await rm(stage, { recursive: true, force: true })
await mkdir(stage)
// Include the real backend and DB sources so the standalone published CLI serves
// the same review API without depending on unpublished workspace crates.
async function bundleRust(sourceDir, targetDir, crateRoot) {
  await mkdir(targetDir, { recursive: true })
  for (const entry of await readdir(sourceDir, { withFileTypes: true })) {
    if (entry.name === 'tests' || entry.name === 'main.rs') continue
    const source = join(sourceDir, entry.name)
    const target = join(targetDir, entry.name === 'lib.rs' ? 'mod.rs' : entry.name)
    if (entry.isDirectory()) {
      await bundleRust(source, target, crateRoot)
    } else if (entry.name.endsWith('.rs')) {
      let content = (await readFile(source, 'utf8'))
        .replace(/\n\/\/ tests -+[\s\S]*$/, '\n')
        .replace(/\bcrate::/g, `${crateRoot}::`)
      if (entry.name === 'lib.rs' && crateRoot === 'crate::backend') {
        content = content.replace('pub use lgtm_db as database;', 'pub mod database;')
      }
      await writeFile(target, content)
    }
  }
}
await bundleRust(join(root, 'apps/lgtm-backend/src'), join(stage, 'backend'), 'crate::backend')
await bundleRust(join(root, 'libs/lgtm-db/src'), join(stage, 'backend/database'), 'crate::backend::database')
const formatted = Bun.spawnSync(
  ['rustfmt', '--edition', '2024', join(stage, 'backend/mod.rs')],
  { cwd: root, stdout: 'inherit', stderr: 'inherit' },
)
if (formatted.exitCode !== 0) throw new Error('Bundled backend formatting failed')
await cp(generated, join(stage, 'proto'), { recursive: true })
await cp(join(root, 'apps/lgtm-web/build'), join(stage, 'web'), { recursive: true })
// Fail before replacing the previous bundle if the web build is incomplete.
await readFile(join(stage, 'web/index.html'))
await writeFile(
  join(stage, 'README.md'),
  'Generated from lgtm-backend and lgtm-web by the CLI bundle task.\n',
)
await rm(join(cli, 'bundle'), { recursive: true, force: true })
await rename(stage, join(cli, 'bundle'))
console.log('Bundled backend source, generated Protobuf types, and static web UI into lgtm-cli')
