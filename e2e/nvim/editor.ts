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
  async choose(...choices: (string | number)[]) {
    await this.lua('_G.e2e.choices = ...', choices)
  }
  async wait(expression: string, description: string) {
    return waitFor(description, () => this.value(expression), Boolean, this.timeoutMs)
  }
  async edit(path: string) {
    await this.lua('vim.cmd.edit(vim.fn.fnameescape(...))', path)
  }
  async state(): Promise<Json | null> {
    return this.lua(`local s = require("lgtm.context").current; if not s then return vim.NIL end
      return {review=s.review, scope=s.scope, target=s.target, comparison=s.comparison, conversation=s.conversation,
        version=s.version, file=s.file, files=s.files, panes=s.panes, root=s.root, git_root=s.git_root,
        base=s.base, head=s.head, tab=s.tab, kind=s.kind, layout=s.layout, initializing=s.initializing,
        load_error=s.load_error, empty=s.empty, native_explorer=s.native_explorer, temp_dir=s.temp_dir,
        render_ready=s.render_ready, reviewed=s.reviewed, moving=s.moving}`)
  }

  async ready() {
    await this.wait(
      'require("lgtm.context").current and require("lgtm.context").current.render_ready',
      'review render',
    )
    return (await this.state())!
  }

  async loaded() {
    await this.wait(
      'require("lgtm.context").current and require("lgtm.context").current.render_ready and not require("lgtm.context").current.initializing and require("lgtm.context").current.conversation ~= nil',
      'review and conversation ready',
    )
    return (await this.state())!
  }

  async pane(side = 'new', line = 1) {
    await this.lua(
      `local side, line = ...; local p = require("lgtm.context").current.panes[side]
      vim.api.nvim_set_current_win(p.win); vim.api.nvim_win_set_cursor(p.win, {line,0})`,
      side,
      line,
    )
  }

  async show(path: string) {
    await this.lua('require("lgtm.context").current.show_file(...)', path)
    await waitFor(
      `selected ${path}`,
      () => this.state(),
      (s) => Boolean(s?.render_ready && s?.file?.path === path),
    )
  }

  async composer() {
    await this.wait(
      'require("lgtm.composer").last and vim.api.nvim_win_is_valid(require("lgtm.composer").last.win)',
      'composer open',
    )
    return this.lua('local c=require("lgtm.composer").last; return {buf=c.buf,win=c.win}')
  }
  async body(body: string) {
    await this.lua(
      'vim.api.nvim_buf_set_lines(require("lgtm.composer").last.buf, 0, -1, false, vim.split(..., "\\n", {plain=true}))',
      body,
    )
  }
  async compose(body: string) {
    await this.composer()
    await this.body(body)
    await this.keys('<C-s>')
    await this.wait(
      'not vim.api.nvim_win_is_valid(require("lgtm.composer").last.win)',
      'composer saved',
    )
  }

  async comments(count: number) {
    return waitFor(
      `${count} comments`,
      () => this.state(),
      (s) => (s?.conversation?.comments?.length ?? 0) === count,
      this.timeoutMs,
    )
  }

  async marks(side = 'new'): Promise<Json[]> {
    return this.lua(
      `local side = ...; local s=require("lgtm.context").current
      return vim.api.nvim_buf_get_extmarks(s.panes[side].buf,require("lgtm.threads").namespace,0,-1,{details=true})`,
      side,
    )
  }

  async text(buf = 0): Promise<string> {
    return this.lua('return table.concat(vim.api.nvim_buf_get_lines(...,0,-1,false),"\\n")', buf)
  }
  async notices(): Promise<Json[]> {
    return this.value('require("lgtm.notifications").history')
  }
  async popup(body?: string) {
    await this.keys('<CR>')
    await this.wait(
      'require("lgtm.threads").last and vim.api.nvim_win_is_valid(require("lgtm.threads").last.win)',
      'thread modal',
    )
    if (body)
      await this.lua(
        `local body=...; local popup=require("lgtm.threads").last
      for row, value in pairs(popup.rows) do if value.comment and value.comment.body==body then
        vim.api.nvim_win_set_cursor(popup.win,{row,0}); return end end
      error("Comment not present: "..body)`,
        body,
      )
    return this.value('require("lgtm.threads").last')
  }

  async openLocal(mode: string, replace = false) {
    if (await this.state()) await this.command('Lgtm close')
    const before = (await this.notices()).length
    await this.choose(mode, ...(replace ? ['Complete saved review and start this comparison'] : []))
    await this.command('Lgtm review')
    const empty =
      mode === 'working'
        ? 'No working-tree changes'
        : mode === 'staged'
          ? 'No staged changes'
          : 'No branch changes'
    await waitFor(
      'local review or explicit empty state',
      async () => ({ state: await this.state(), notices: (await this.notices()).slice(before) }),
      (value) =>
        Boolean(
          value.state?.review &&
          (value.state.render_ready || value.notices.some((n) => n.message === empty)),
        ),
    )
    return (await this.state())!
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
        messages: await this.lua('return vim.api.nvim_exec2("messages",{output=true}).output'),
        notices: await this.notices(),
        state: await this.state(),
        picker: await this.lua(`local p = require("lgtm.picker").last
          if not p then return vim.NIL end
          return {query=p.query,closed=p.closed,index=p.index,mode=vim.fn.mode(),
            text=vim.api.nvim_buf_is_valid(p.buf) and vim.api.nvim_buf_get_lines(p.buf,0,-1,false) or {},
            entries=vim.tbl_map(function(pr) return {number=pr.number,title=pr.title,next_page=pr.nextPage} end,p.entries),
            filtered=vim.tbl_map(function(pr) return {number=pr.number,title=pr.title,next_page=pr.nextPage} end,p.filtered)}`),
        current: await this.lua(
          'return {buf=vim.api.nvim_get_current_buf(),win=vim.api.nvim_get_current_win(),tab=vim.api.nvim_get_current_tabpage(),mapping=vim.fn.maparg("c","n",false,true).desc}',
        ),
      }
    } catch {
      return {
        stderr: this.errors,
        backend: await readFile(join(this.runtime.directory, 'backend.log'), 'utf8'),
      }
    }
  }
}
