import { test as base, expect, type Page } from '@playwright/test'
import { execFileSync, spawn } from 'node:child_process'
import { mkdtemp, mkdir, readFile, realpath, writeFile, rm, stat } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'

export { expect }
export interface Workspace {
  root: string
  lgtmHomeDir: string
  pickedFolder: string | null
  failWrites: boolean
  failLists: boolean
  destroyed: boolean
  quit: boolean
  configuredWindowTheme: 'light' | 'dark' | null
  windowTheme: 'light' | 'dark' | null
  setThemeCalls: Array<'light' | 'dark' | null>
  emit: (event: string, payload?: unknown) => Promise<void>
  createExternalDir: () => Promise<string>
}

interface CommandArgs {
  path?: string
  name?: string
  parentPath?: string
  relativePath?: string
  content?: string
  offset?: number
  languageId?: string
  patch?: unknown
  scope?: string | null
  format?: string | null
}

interface BridgeMessage {
  id?: number
  ok?: unknown
  error?: { message?: string } | string
  event?: string
  payload?: unknown
}

const repoRoot = fileURLToPath(new URL('../..', import.meta.url))
const desktopBinary = join(
  repoRoot,
  'apps',
  'lgtm-desktop',
  'target',
  'debug',
  process.platform === 'win32' ? 'lgtm-desktop.exe' : 'lgtm-desktop',
)

const MOCK_LSP_SERVER_SOURCE = `import { Buffer } from 'node:buffer';

let buffer = Buffer.alloc(0);
const openDocs = new Map();

function send(msg) {
  const body = Buffer.from(JSON.stringify(msg), 'utf8');
  process.stdout.write(\`Content-Length: \${body.length}\\r\\n\\r\\n\`);
  process.stdout.write(body);
}

function publishDiagnostics(uri, text) {
  const diagnostics = [];
  const lines = text.split('\\n');
  const markers = ['BROKEN', 'invalid-theme', 'unknownLocalField'];
  for (let i = 0; i < lines.length; i++) {
    for (const marker of markers) {
      const idx = lines[i].indexOf(marker);
      if (idx !== -1) {
        const startChar =
          marker === 'BROKEN' ? idx : lines[i].length - lines[i].trimStart().length;
        const endChar = marker === 'BROKEN' ? idx + marker.length : lines[i].length;
        diagnostics.push({
          range: {
            start: { line: i, character: startChar },
            end: { line: i, character: endChar },
          },
          severity: 1,
          source: 'rustc',
          code: 'E0425',
          message:
            marker === 'BROKEN'
              ? 'cannot find value \`BROKEN\` in this scope'
              : \`invalid value or symbol \\\`\${marker}\\\`\`,
        });
      }
    }
  }
  send({
    jsonrpc: '2.0',
    method: 'textDocument/publishDiagnostics',
    params: { uri, diagnostics },
  });
}

function handleMessage(msg) {
  if (msg.method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        capabilities: {
          textDocumentSync: 1,
          hoverProvider: true,
          definitionProvider: true,
          completionProvider: {
            triggerCharacters: ['.', ':'],
          },
          signatureHelpProvider: {
            triggerCharacters: ['(', ','],
          },
        },
      },
    });
    return;
  }
  if (msg.method === 'textDocument/didOpen') {
    const { uri, text } = msg.params.textDocument;
    openDocs.set(uri, text);
    publishDiagnostics(uri, text);
    return;
  }
  if (msg.method === 'textDocument/didChange') {
    const { uri } = msg.params.textDocument;
    const text = msg.params.contentChanges[0].text;
    openDocs.set(uri, text);
    publishDiagnostics(uri, text);
    return;
  }
  if (msg.method === 'textDocument/didClose') {
    openDocs.delete(msg.params.textDocument.uri);
    return;
  }
  if (msg.method === 'textDocument/hover') {
    const uri = msg.params.textDocument.uri;
    const isConfig = uri.endsWith('/.lgtm/config.json') || uri.endsWith('\\\\.lgtm\\\\config.json');
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        contents: {
          kind: 'markdown',
          value: isConfig
            ? 'Resolved appearance settings section \\\\(\\\`appearance\\\\.\\\\*\\\`\\\\)\\\\.'
            : '\`\`\`rust\\npub fn helper() -> i32\\n\`\`\`\\n\\n---\\n\\nComputes the **workspace** answer using \`helper\`.',
        },
        range: {
          start: { line: msg.params.position.line, character: 0 },
          end: { line: msg.params.position.line, character: 6 },
        },
      },
    });
    return;
  }
  if (msg.method === 'textDocument/definition') {
    const currentUri = msg.params.textDocument.uri;
    for (const [docUri, docText] of openDocs.entries()) {
      const lines = docText.split('\\n');
      for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
        const pubIdx = lines[lineIdx].indexOf('pub fn helper');
        if (pubIdx !== -1 && docUri !== currentUri) {
          send({
            jsonrpc: '2.0',
            id: msg.id,
            result: [
              {
                uri: docUri,
                range: {
                  start: { line: lineIdx, character: pubIdx + 7 },
                  end: { line: lineIdx, character: pubIdx + 13 },
                },
              },
            ],
          });
          return;
        }
      }
    }
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: [
        {
          uri: currentUri,
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 6 },
          },
        },
      ],
    });
    return;
  }
  if (msg.method === 'textDocument/completion') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        isIncomplete: false,
        items: [
          {
            label: 'helper_value',
            kind: 3,
            detail: 'fn helper_value() -> i32',
            documentation: {
              kind: 'markdown',
              value: 'Returns the **helper** value.',
            },
            insertText: 'helper_value()',
          },
        ],
      },
    });
    return;
  }
  if (msg.method === 'textDocument/signatureHelp') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        signatures: [
          {
            label: 'fn helper(x: i32, y: i32) -> i32',
            documentation: {
              kind: 'markdown',
              value: 'Computes the sum of \`x\` and \`y\`.',
            },
            parameters: [{ label: 'x: i32' }, { label: 'y: i32' }],
          },
        ],
        activeSignature: 0,
        activeParameter: 0,
      },
    });
    return;
  }
  if (msg.method === 'shutdown') {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
    return;
  }
  if (msg.method === 'exit') {
    process.exit(0);
  }
}

process.stdin.on('data', (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (true) {
    const headerEnd = buffer.indexOf('\\r\\n\\r\\n');
    if (headerEnd === -1) break;
    const header = buffer.slice(0, headerEnd).toString('utf8');
    const match = header.match(/Content-Length:\\s*(\\d+)/i);
    if (!match) break;
    const length = Number(match[1]);
    const total = headerEnd + 4 + length;
    if (buffer.length < total) break;
    const body = buffer.slice(headerEnd + 4, total).toString('utf8');
    buffer = buffer.slice(total);
    handleMessage(JSON.parse(body));
  }
});
`

async function emit(page: Page, event: string, payload?: unknown) {
  if (page.isClosed()) return
  await page
    .evaluate(
      async ({ name, data }) => {
        await (
          window as unknown as {
            workspaceTestEmit: (event: string, payload?: unknown) => Promise<void>
          }
        ).workspaceTestEmit(name, data)
      },
      { name: event, data: payload },
    )
    .catch(() => {})
}

// Fixtures --------------------------------------------------------------------
// The browser runs the production UI and routes every Tauri command through the
// real `lgtm-desktop --ipc-bridge` Rust binary (`app::commands`, `WorkspaceService`,
// `ConfigService`, and `LspService`).
export const test = base.extend<{
  workspace: Workspace
  initialWorkspace: 'empty' | 'existing' | 'default-seeded'
}>({
  initialWorkspace: ['existing', { option: true }],
  workspace: [
    async ({ page, initialWorkspace }, use, testInfo) => {
      const homeRoot = await mkdtemp(join(tmpdir(), 'lgtm-ui-'))
      const lgtmHomeDir = join(homeRoot, '.lgtm')
      const initialRoot = join(lgtmHomeDir, 'workspace')
      const extraDirs: string[] = []

      await mkdir(lgtmHomeDir, { recursive: true })
      const mockLspScript = join(homeRoot, 'mock-lsp.mjs')
      await writeFile(mockLspScript, MOCK_LSP_SERVER_SOURCE, 'utf8')

      const tauriConfRaw = await readFile(
        join(repoRoot, 'apps/lgtm-desktop/tauri.conf.json'),
        'utf8',
      )
      const tauriConf = JSON.parse(tauriConfRaw) as {
        app?: { windows?: Array<{ theme?: string }> }
      }
      const configuredTheme = tauriConf.app?.windows?.[0]?.theme?.toLowerCase()
      const initialWindowTheme: 'light' | 'dark' | null =
        configuredTheme === 'light' || configuredTheme === 'dark' ? configuredTheme : null

      const workspace: Workspace = {
        root: initialRoot,
        lgtmHomeDir,
        pickedFolder: null,
        failLists: false,
        failWrites: false,
        destroyed: false,
        quit: false,
        configuredWindowTheme: initialWindowTheme,
        windowTheme: initialWindowTheme,
        setThemeCalls: [],
        emit: (event, payload) => emit(page, event, payload),
        createExternalDir: async () => {
          const dir = await mkdtemp(join(tmpdir(), 'lgtm-external-repo-'))
          extraDirs.push(dir)
          return dir
        },
      }

      if (initialWorkspace === 'existing') {
        await mkdir(join(initialRoot, 'Drafts'), { recursive: true })
        await mkdir(join(initialRoot, 'Empty'), { recursive: true })
        await writeFile(join(initialRoot, 'Drafts/one.md'), '# One\n\nFirst file.\n')
        await writeFile(join(initialRoot, 'Drafts/two.md'), '# Two\n\nSecond file.\n')
        await writeFile(join(initialRoot, 'same.md'), '# Markdown\n')
        await writeFile(join(initialRoot, 'notes.txt'), 'Plain text note.\n')
        await writeFile(join(initialRoot, '.gitignore'), 'target/\n')
      } else if (initialWorkspace === 'default-seeded') {
        await mkdir(initialRoot, { recursive: true })
        await writeFile(join(initialRoot, 'hello.py'), 'def hello_world():\n    BROKEN()\n', 'utf8')
        await writeFile(
          join(initialRoot, 'hello.ts'),
          'export function helloWorld() {\n  console.log("hello world!");\n}\n',
          'utf8',
        )
      }

      const bridge = spawn(desktopBinary, ['--ipc-bridge', homeRoot], {
        env: {
          ...process.env,
          LGTM_IPC_BRIDGE: '1',
          LGTM_E2E_PERMANENT_DELETE: '1',
          LGTM_E2E_LSP_SCRIPT: mockLspScript,
        },
        stdio: ['pipe', 'pipe', 'inherit'],
      })

      let nextId = 1
      const pending = new Map<
        number,
        { resolve: (value: unknown) => void; reject: (reason: Error) => void }
      >()

      const rl = createInterface({ input: bridge.stdout })
      rl.on('line', (line) => {
        const trimmed = line.trim()
        if (!trimmed) return
        let msg: BridgeMessage
        try {
          msg = JSON.parse(trimmed) as BridgeMessage
        } catch {
          return
        }
        if (msg.event) {
          void emit(page, msg.event, msg.payload)
          return
        }
        if (typeof msg.id === 'number') {
          const waiter = pending.get(msg.id)
          if (!waiter) return
          pending.delete(msg.id)
          if ('error' in msg && msg.error !== undefined) {
            const errMsg =
              typeof msg.error === 'object' && msg.error && 'message' in msg.error
                ? String(msg.error.message)
                : String(msg.error)
            waiter.reject(new Error(errMsg))
          } else {
            waiter.resolve(msg.ok)
          }
        }
      })

      function invokeBridge(command: string, args: CommandArgs): Promise<unknown> {
        const id = nextId++
        return new Promise((resolvePromise, rejectPromise) => {
          pending.set(id, { resolve: resolvePromise, reject: rejectPromise })
          bridge.stdin.write(`${JSON.stringify({ id, command, args })}\n`)
        })
      }

      try {
        await page.exposeFunction(
          'workspaceTestInvoke',
          async (command: string, args: CommandArgs = {}) => {
            if (command === 'workspace_pick_folder') {
              return workspace.pickedFolder
            }
            if (command === 'workspace_list' && workspace.failLists) {
              throw new Error('Folder unavailable')
            }
            if (
              (command === 'workspace_create_folder' || command === 'workspace_create_file') &&
              workspace.failWrites
            ) {
              throw new Error('Cannot create a file or folder. Disk unavailable.')
            }
            if (
              (command === 'workspace_rename' ||
                command === 'workspace_delete' ||
                command === 'workspace_write') &&
              workspace.failWrites
            ) {
              throw new Error('Disk unavailable')
            }
            if (command === 'plugin:window|destroy') {
              workspace.destroyed = true
              return null
            }
            if (command === 'plugin:window|set_theme') {
              const rawTheme = args as {
                theme?: 'light' | 'dark' | null
                value?: 'light' | 'dark' | null
              }
              const nextTheme = rawTheme.theme ?? rawTheme.value ?? null
              workspace.windowTheme = nextTheme
              workspace.setThemeCalls.push(nextTheme)
              return null
            }
            if (command === 'plugin:window|theme') {
              if (workspace.windowTheme) {
                return workspace.windowTheme
              }
              const prefersDark = await page.evaluate(
                () => window.matchMedia('(prefers-color-scheme: dark)').matches,
              )
              return prefersDark ? 'dark' : 'light'
            }

            const result = await invokeBridge(command, args)
            if (
              command === 'workspace_open' &&
              result &&
              typeof result === 'object' &&
              'root' in result &&
              typeof (result as { root: unknown }).root === 'string'
            ) {
              workspace.root = (result as { root: string }).root
            }
            if (command === 'workspace_finish_quit') {
              workspace.quit = true
            }
            return result
          },
        )
        if (initialWindowTheme) {
          await page.emulateMedia({ colorScheme: initialWindowTheme })
        }
        await page.goto('/e2e/desktop/fixture.html')
        await expect(page.getByRole('button', { name: 'New file', exact: true })).toBeEnabled()
        await expect(page.getByText('No file selected')).toBeVisible()
        await use(workspace)
      } finally {
        try {
          if (testInfo.status !== testInfo.expectedStatus && !page.isClosed()) {
            await testInfo.attach('failure', {
              body: await page.screenshot({ timeout: 2000 }),
              contentType: 'image/png',
            })
          }
        } finally {
          try {
            await page.close()
          } finally {
            try {
              rl.close()
              bridge.stdin.end()
              bridge.kill()
            } finally {
              await rm(homeRoot, { recursive: true, force: true })
              for (const dir of extraDirs) {
                await rm(dir, { recursive: true, force: true })
              }
            }
          }
        }
      }
    },
    { auto: true },
  ],
})

export function entry(page: Page, path: string) {
  return page.locator(`[role="treeitem"][data-value=${JSON.stringify(path)}]`)
}

export async function createFile(page: Page) {
  const selected = page.locator('[role="treeitem"][aria-selected="true"]')
  const previous = (await selected.count()) ? await selected.getAttribute('data-value') : null
  await page.getByRole('button', { name: 'New file', exact: true }).click()
  await expect.poll(() => selected.getAttribute('data-value')).not.toBe(previous)
  await expect(page.locator('.cm-content')).toBeVisible()
}

export async function contextAction(page: Page, path: string, action: string) {
  const parts = path.split('/')
  for (let depth = 1; depth < parts.length; depth++) {
    const parent = entry(page, parts.slice(0, depth).join('/'))
    if ((await parent.getAttribute('aria-expanded')) === 'false') {
      await parent.getByRole('button', { name: parts[depth - 1]!, exact: true }).click()
    }
  }
  const node = entry(page, path)
  const control = node.getByRole('button', { name: path.split('/').at(-1)!, exact: true })
  await ((await control.count()) ? control : node).click({ button: 'right' })
  await page.getByRole('menuitem', { name: action, exact: action !== 'Rename' }).click()
}

export async function renameEntry(page: Page, path: string, name: string) {
  await contextAction(page, path, 'Rename')
  const input = page.getByRole('textbox', { name: 'Name', exact: true })
  await expect(input).toBeFocused()
  await input.fill(name)
  await input.press('Enter')
  await expect(input).toHaveCount(0)
  const parent = path.includes('/') ? path.slice(0, path.lastIndexOf('/') + 1) : ''
  await expect(entry(page, `${parent}${name}`)).toBeVisible()
}

export async function createFolder(page: Page, name: string) {
  await page.getByRole('button', { name: 'New folder', exact: true }).click()
  const input = page.getByRole('textbox', { name: 'Name', exact: true })
  await expect(input).toBeFocused()
  await input.fill(name)
  await input.press('Enter')
  await expect(input).toHaveCount(0)
}

export async function deleteEntry(page: Page, path: string) {
  await contextAction(page, path, 'Move to Trash')
  await expect(entry(page, path)).toHaveCount(0)
}

export async function exists(path: string) {
  return stat(path).then(
    () => true,
    (error: NodeJS.ErrnoException) => {
      if (error.code === 'ENOENT') return false
      throw error
    },
  )
}

export async function createGitRepo(
  dir: string,
  files: Record<string, string> = { 'README.md': '# Repo\n' },
): Promise<string> {
  await mkdir(dir, { recursive: true })
  const canonical = await realpath(dir)
  execFileSync('git', ['init', '-b', 'main'], { cwd: canonical, stdio: 'ignore' })
  execFileSync('git', ['config', 'user.email', 'test@lgtm.dev'], {
    cwd: canonical,
    stdio: 'ignore',
  })
  execFileSync('git', ['config', 'user.name', 'LGTM Test'], {
    cwd: canonical,
    stdio: 'ignore',
  })
  for (const [relPath, content] of Object.entries(files)) {
    const fullPath = join(canonical, relPath)
    const lastSlash = fullPath.lastIndexOf('/')
    if (lastSlash !== -1) {
      await mkdir(fullPath.slice(0, lastSlash), { recursive: true })
    }
    await writeFile(fullPath, content, 'utf8')
  }
  execFileSync('git', ['add', '.'], { cwd: canonical, stdio: 'ignore' })
  execFileSync('git', ['commit', '-m', 'Initial commit'], { cwd: canonical, stdio: 'ignore' })
  return canonical
}

export async function createExternalWorktree(
  repoDir: string,
  worktreeDir: string,
  branch: string,
): Promise<string> {
  await rm(worktreeDir, { recursive: true, force: true })
  execFileSync('git', ['worktree', 'add', '-b', branch, worktreeDir, 'HEAD'], {
    cwd: repoDir,
    stdio: 'ignore',
  })
  return realpath(worktreeDir)
}
