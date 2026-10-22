import { execFile } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { promisify } from 'node:util'
import { isolatedEnv } from '../common/test-utils.js'

/** Fail before creating GitHub fixtures if either real editor profile cannot start. */
export default async function verifyDependencies() {
  const home = await mkdtemp(join(tmpdir(), 'lgtm-editor-preflight-'))
  const exec = promisify(execFile)
  const verify = `lua local ok, err = pcall(function()
    assert(package.loaded['lgtm'], vim.v.errmsg ~= '' and vim.v.errmsg or 'LGTM initialization failed')
    assert(vim.v.errmsg == '', vim.v.errmsg)
    assert(require('codediff.core.diff').compute_diff({'before'}, {'after'}), 'Native diff failed')
  end); if not ok then vim.api.nvim_err_writeln(tostring(err)); vim.cmd('cquit 1') end`
  try {
    for (const kak of ['0', '1']) {
      try {
        await exec(
          'nvim',
          [
            '--headless',
            '-u',
            join(import.meta.dirname, 'init.lua'),
            '-i',
            'NONE',
            '-n',
            '-c',
            verify,
            '-c',
            'qa!',
          ],
          {
            env: {
              ...isolatedEnv(home),
              LGTM_REPO_ROOT: resolve(import.meta.dirname, '../..'),
              LGTM_E2E_KAK: kak,
            },
            timeout: 60_000,
          },
        )
      } catch (error) {
        const output = error as Error & { stdout?: string; stderr?: string }
        throw new Error(
          `Editor preflight failed (${kak === '1' ? 'Kak' : 'required'} profile). Run premise run //e2e/nvim:install.\n${output.stdout ?? ''}\n${output.stderr ?? ''}`,
          { cause: error },
        )
      }
    }
  } finally {
    await rm(home, { recursive: true, force: true })
  }
}
