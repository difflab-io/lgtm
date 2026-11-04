import { test as base, expect, type Page } from '@playwright/test'
import { spawn } from 'node:child_process'
import { mkdtemp, mkdir, writeFile, rm, stat } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'

export { expect }
export interface Workspace {
  root: string
  pickedFolder: string | null
  failWrites: boolean
  failLists: boolean
  destroyed: boolean
  quit: boolean
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
  for (let i = 0; i < lines.length; i++) {
    const idx = lines[i].indexOf('BROKEN');
    if (idx !== -1) {
      diagnostics.push({
        range: {
          start: { line: i, character: idx },
          end: { line: i, character: idx + 6 },
        },
        severity: 1,
        source: 'rustc',
        code: 'E0425',
        message: 'cannot find value \`BROKEN\` in this scope',
      });
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
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        contents: {
          kind: 'markdown',
          value: 'fn helper() -> i32\\nComputes the workspace answer.',
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
      const escapedScript = mockLspScript.replace(/\\/g, '\\\\')
      await writeFile(
        join(lgtmHomeDir, 'config.toml'),
        [
          '[languages.rust]',
          'enabled = true',
          'extensions = ["rs"]',
          'root_markers = ["Cargo.toml"]',
          'mise_tools = {}',
          'command = "node"',
          `args = ["${escapedScript}"]`,
          '',
          '[languages.typescript]',
          'enabled = true',
          'extensions = ["ts", "tsx"]',
          'root_markers = ["tsconfig.json", "package.json"]',
          'mise_tools = {}',
          'command = "node"',
          `args = ["${escapedScript}"]`,
          '',
          '[languages.javascript]',
          'enabled = true',
          'extensions = ["js", "jsx"]',
          'root_markers = ["package.json", "jsconfig.json"]',
          'mise_tools = {}',
          'command = "node"',
          `args = ["${escapedScript}"]`,
          '',
          '[languages.python]',
          'enabled = true',
          'extensions = ["py"]',
          'root_markers = ["pyproject.toml", "requirements.txt"]',
          'mise_tools = {}',
          'command = "node"',
          `args = ["${escapedScript}"]`,
          '',
          '[languages.go]',
          'enabled = true',
          'extensions = ["go"]',
          'root_markers = ["go.mod"]',
          'mise_tools = {}',
          'command = "node"',
          `args = ["${escapedScript}"]`,
          '',
        ].join('\n'),
        'utf8',
      )

      const workspace: Workspace = {
        root: initialRoot,
        pickedFolder: null,
        failLists: false,
        failWrites: false,
        destroyed: false,
        quit: false,
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
