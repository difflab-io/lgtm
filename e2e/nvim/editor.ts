import { spawn, type ChildProcess } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import { appendFileSync } from 'node:fs'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { decodeMultiStream, encode } from '@msgpack/msgpack'
import { ROOT, TestHarness, stop } from '../common/test-harness.js'
import { waitFor, type Json } from '../common/test-utils.js'

/** Operates a real editor. Helpers perform actions; specifications own assertions. */
export class Editor {
  private child?: ChildProcess
  private errors = ''
  timeoutMs = 10_000
  private sequence = 0
  private pending = new Map<
    number,
    { resolve: (value: unknown) => void; reject: (error: Error) => void }
  >()
  constructor(
    readonly runtime: TestHarness,
    readonly profile: 'required' | 'kak',
  ) {}

  async start() {
    this.errors = ''
    this.child = spawn(
      'nvim',
      ['--embed', '--headless', '-u', join(ROOT, 'e2e/nvim/init.lua'), '-i', 'NONE', '-n'],
      {
        cwd: this.runtime.workspace,
        env: {
          ...this.runtime.env,
          LGTM_REPO_ROOT: ROOT,
          LGTM_E2E_KAK: this.profile === 'kak' ? '1' : '0',
          LGTM_RPC_URL: this.runtime.endpoint,
          PATH: join(ROOT, 'target/debug') + ':' + process.env.PATH,
        },
        stdio: ['pipe', 'pipe', 'pipe'],
      },
    )
    this.child.stderr?.on('data', (chunk) => {
      this.errors += chunk
      appendFileSync(join(this.runtime.directory, 'editor.log'), chunk)
    })
    this.child.once('exit', () => {
      for (const request of this.pending.values()) request.reject(new Error('Editor exited'))
      this.pending.clear()
    })
    void (async () => {
      for await (const message of decodeMultiStream(this.child!.stdout!)) {
        const [kind, id, error, value] = message as [number, number, unknown, unknown]
        if (kind !== 1) continue // UI redraws are notifications; the editor renders the real screen.
        const request = this.pending.get(id)
        if (!request) continue
        this.pending.delete(id)
        if (error) request.reject(new Error(`Neovim: ${JSON.stringify(error)}`))
        else request.resolve(value)
      }
    })().catch((error) => {
      this.errors += String(error)
      for (const request of this.pending.values()) request.reject(error)
      this.pending.clear()
    })
    await waitFor('editor initialization', () => this.value('package.loaded["lgtm"] ~= nil'))
    // A UI is required for screen positions, float sizes and real key handling.
    await this.request('nvim_ui_attach', [120, 40, { rgb: true, ext_linegrid: true }])
  }

  async lua<T = Json>(code: string, ...args: unknown[]): Promise<T> {
    return this.request<T>('nvim_exec_lua', [code, args])
  }

  private async request<T>(method: string, args: unknown[]): Promise<T> {
    const id = ++this.sequence
    let timer: ReturnType<typeof setTimeout> | undefined
    try {
      return await new Promise<T>((resolve, reject) => {
        this.pending.set(id, { resolve: (value) => resolve(value as T), reject })
        timer = setTimeout(() => reject(new Error(`Editor RPC timed out: ${method}`)), 5000)
        this.child!.stdin!.write(encode([0, id, method, args]))
      })
    } finally {
      clearTimeout(timer)
      this.pending.delete(id)
    }
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async value<T = any>(expression: string, ...args: unknown[]): Promise<T> {
    return this.lua<T>(`return ${expression}`, ...args)
  }

  async command(command: string) {
    await this.request('nvim_command', [command])
  }

  /** Key input is asynchronous; callers wait for each operation’s observable result. */
  async keys(keys: string) {
    await this.request('nvim_input', [keys])
    await delay(35)
  }

  async wait(expression: string, description: string) {
    return waitFor(description, () => this.value(expression), Boolean, this.timeoutMs)
  }

  async edit(path: string) {
    await this.lua('vim.cmd.edit(vim.fn.fnameescape(...))', path)
  }

  async text(buf = 0): Promise<string> {
    return this.lua('return table.concat(vim.api.nvim_buf_get_lines(...,0,-1,false),"\\n")', buf)
  }

  async close() {
    await stop(this.child)
  }

  async diagnostics() {
    if (!this.child || this.child.exitCode !== null || this.child.signalCode)
      return { stderr: this.errors, editor: 'stopped' }
    try {
      return {
        stderr: this.errors,
        screen: await this.lua(`vim.cmd("redraw")
          local rows = {}
          for row = 1, vim.o.lines do
            local cells = {}
            for col = 1, vim.o.columns do
              cells[#cells + 1] = vim.fn.screenstring(row, col)
            end
            rows[#rows + 1] = table.concat(cells)
          end
          return table.concat(rows, "\\n")`),
        windows: await this.lua(`return vim.tbl_map(function(win)
          return {win=win, buf=vim.api.nvim_win_get_buf(win),
            view=vim.api.nvim_win_call(win, vim.fn.winsaveview)}
          end, vim.api.nvim_tabpage_list_wins(0))`),
        messages: await this.lua('return vim.api.nvim_exec2("messages",{output=true}).output'),
        current: await this.lua(
          'return {buf=vim.api.nvim_get_current_buf(),win=vim.api.nvim_get_current_win(),tab=vim.api.nvim_get_current_tabpage()}',
        ),
      }
    } catch {
      return {
        stderr: this.errors,
        backend: await readFile(join(this.runtime.directory, 'backend.log'), 'utf8').catch(
          () => '',
        ),
      }
    }
  }
}
