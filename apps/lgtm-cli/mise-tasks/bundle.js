#!/usr/bin/env bun
//MISE description="Bundle the private backend and web UI into the publishable CLI"
//MISE depends=["//apps/lgtm-backend:build", "//apps/lgtm-web:build"]

import { cp, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises'
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
const backendSource = await readFile(join(root, 'apps/lgtm-backend/src/lib.rs'), 'utf8')
// Bundle the existing RPC router, not the backend's review-domain/database modules.
// Future CLI review commands will use the generated RPC client.
await writeFile(
  join(stage, 'backend.rs'),
  backendSource
    .replace(/^pub use lgtm_db as database;\n|^pub mod (?:error|models|services);\n/gm, '')
    .replace(/\n{3,}/g, '\n\n'),
)
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
