import { test as base, expect, type Page } from '@playwright/test'
import { mkdtemp, mkdir, readFile, writeFile, readdir, rm, rename, stat } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve, sep } from 'node:path'
import { createHash } from 'node:crypto'

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

interface MockDiagnostic {
  from: number
  to: number
  line: number
  character: number
  endLine: number
  endCharacter: number
  severity: 'error' | 'warning' | 'info' | 'hint'
  message: string
  source?: string
  code?: string
}

interface MockServer {
  workspaceRoot: string
  languageId: string
  projectRoot: string
  status: 'starting' | 'ready' | 'unavailable' | 'error'
  command: string
  message?: string | null
  errorCount: number
  warningCount: number
}

const revision = (content: string) => createHash('sha256').update(content).digest('hex')

async function emit(page: Page, event: string, payload?: unknown) {
  await page.evaluate(
    async ({ name, data }) => {
      await (
        window as unknown as {
          workspaceTestEmit: (event: string, payload?: unknown) => Promise<void>
        }
      ).workspaceTestEmit(name, data)
    },
    { name: event, data: payload },
  )
}

function detectLanguage(relativePath: string): { languageId: string; command: string } | null {
  if (relativePath.endsWith('.rs')) return { languageId: 'rust', command: 'rust-analyzer' }
  if (relativePath.endsWith('.ts') || relativePath.endsWith('.tsx'))
    return { languageId: 'typescript', command: 'typescript-language-server' }
  if (relativePath.endsWith('.py')) return { languageId: 'python', command: 'pyright-langserver' }
  if (relativePath.endsWith('.go')) return { languageId: 'go', command: 'gopls' }
  return null
}

function computeDiagnostics(text: string): MockDiagnostic[] {
  const diagnostics: MockDiagnostic[] = []
  const lines = text.split('\n')
  let lineStartOffset = 0
  for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
    const lineText = lines[lineIdx]!
    const brokenIdx = lineText.indexOf('BROKEN')
    if (brokenIdx !== -1) {
      diagnostics.push({
        from: lineStartOffset + brokenIdx,
        to: lineStartOffset + brokenIdx + 6,
        line: lineIdx,
        character: brokenIdx,
        endLine: lineIdx,
        endCharacter: brokenIdx + 6,
        severity: 'error',
        message: 'cannot find value `BROKEN` in this scope',
        source: 'rustc',
        code: 'E0425',
      })
    }
    lineStartOffset += lineText.length + 1
  }
  return diagnostics
}

// Fixtures --------------------------------------------------------------------
// The browser runs the production UI. Only the native IPC transport is substituted;
// workspace contents and persistence assertions use real, test-owned directories.
export const test = base.extend<{ workspace: Workspace; initialWorkspace: 'empty' | 'existing' }>({
  initialWorkspace: ['existing', { option: true }],
  workspace: [
    async ({ page, initialWorkspace }, use, testInfo) => {
      const homeRoot = await mkdtemp(join(tmpdir(), 'lgtm-ui-'))
      const initialRoot =
        initialWorkspace === 'empty' ? join(homeRoot, '.lgtm', 'workspace') : homeRoot
      const extraDirs: string[] = []
      const servers = new Map<string, MockServer>()
      const diagnosticsByPath = new Map<string, { languageId: string; items: MockDiagnostic[] }>()
      const openDocs = new Map<string, string>()

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
      function location(...parts: string[]) {
        const target = resolve(workspace.root, ...parts)
        if (target !== workspace.root && !target.startsWith(workspace.root + sep))
          throw new Error('Test path leaves its workspace')
        return target
      }
      function syncServerCounts(languageId: string, command: string): MockServer {
        let errorCount = 0
        let warningCount = 0
        for (const entry of diagnosticsByPath.values()) {
          if (entry.languageId === languageId) {
            for (const d of entry.items) {
              if (d.severity === 'error') errorCount++
              if (d.severity === 'warning') warningCount++
            }
          }
        }
        const srv: MockServer = {
          workspaceRoot: workspace.root,
          languageId,
          projectRoot: workspace.root,
          status: 'ready',
          command,
          message: null,
          errorCount,
          warningCount,
        }
        servers.set(languageId, srv)
        return srv
      }

      try {
        if (initialWorkspace === 'existing') {
          await mkdir(location('Drafts'))
          await mkdir(location('Empty'))
          await writeFile(location('Drafts/one.md'), '# One\n\nFirst file.\n')
          await writeFile(location('Drafts/two.md'), '# Two\n\nSecond file.\n')
          await writeFile(location('same.md'), '# Markdown\n')
          await writeFile(location('notes.txt'), 'Plain text note.\n')
        }

        async function list(path: string) {
          const entries = await readdir(location(path), { withFileTypes: true })
          return entries
            .filter((entry) => entry.name !== '.lgtm' && !entry.name.startsWith('.lgtm-save-'))
            .map((entry) => ({
              name: entry.name,
              path: path ? `${path}/${entry.name}` : entry.name,
              kind: entry.isDirectory() ? 'directory' : 'file',
            }))
            .sort(
              (a, b) =>
                Number(a.kind !== 'directory') - Number(b.kind !== 'directory') ||
                a.name.localeCompare(b.name),
            )
        }

        await page.exposeFunction(
          'workspaceTestInvoke',
          async (command: string, args: CommandArgs = {}) => {
            if (command === 'workspace_open') {
              if (args.path) {
                const candidate = resolve(args.path)
                const info = await stat(candidate).catch(() => null)
                if (!info?.isDirectory()) throw new Error('Choose an existing folder.')
                workspace.root = candidate
              } else {
                await mkdir(workspace.root, { recursive: true })
              }
              servers.clear()
              diagnosticsByPath.clear()
              openDocs.clear()
              return { root: workspace.root, entries: await list('') }
            }
            if (command === 'workspace_pick_folder') {
              return workspace.pickedFolder
            }
            if (command === 'workspace_list') {
              if (workspace.failLists) throw new Error('Folder unavailable')
              return list(args.relativePath!)
            }
            if (command === 'workspace_create_folder' || command === 'workspace_create_file') {
              if (workspace.failWrites)
                throw new Error('Cannot create a file or folder. Disk unavailable.')
              let name = args.name!.trim()
              if (!name || /[\\/:]/.test(name) || name === '.' || name === '..')
                throw new Error('Invalid name')
              const parent = args.parentPath ?? ''
              const kind = command === 'workspace_create_folder' ? 'directory' : 'file'
              if (kind === 'file' && !name.includes('.')) name += '.md'
              const path = parent ? `${parent}/${name}` : name
              try {
                if (kind === 'directory') await mkdir(location(path))
                else
                  await writeFile(location(path), `# ${name.replace(/\.[^.]+$/, '')}\n\n`, {
                    flag: 'wx',
                  })
              } catch (error) {
                if ((error as NodeJS.ErrnoException).code === 'EEXIST')
                  throw new Error('A file or folder with this name already exists.', {
                    cause: error,
                  })
                throw error
              }
              return { path, name, kind }
            }
            if (command === 'workspace_rename') {
              if (workspace.failWrites) throw new Error('Disk unavailable')
              const before = args.relativePath!
              const parent = before.includes('/') ? before.slice(0, before.lastIndexOf('/')) : ''
              const path = parent ? `${parent}/${args.name}` : args.name!
              if (before !== path) {
                const targetExists = await stat(location(path)).then(
                  () => true,
                  () => false,
                )
                if (targetExists) throw new Error('A file or folder with this name already exists.')
                await rename(location(before), location(path))
              }
              return {
                path,
                name: args.name!,
                kind: (await stat(location(path))).isDirectory() ? 'directory' : 'file',
              }
            }
            if (command === 'workspace_delete') {
              if (workspace.failWrites) throw new Error('Disk unavailable')
              await rm(location(args.relativePath!), { recursive: true })
              diagnosticsByPath.delete(args.relativePath!)
              openDocs.delete(args.relativePath!)
              return null
            }
            if (command === 'workspace_read') {
              const rel = args.relativePath!
              const content = await readFile(location(rel), 'utf8')
              openDocs.set(rel, content)
              const detected = detectLanguage(rel)
              if (!detected) {
                return { content, revision: revision(content) }
              }
              const items = computeDiagnostics(content)
              diagnosticsByPath.set(rel, { languageId: detected.languageId, items })
              const lspStatus = syncServerCounts(detected.languageId, detected.command)
              return {
                content,
                revision: revision(content),
                languageId: detected.languageId,
                lspStatus,
              }
            }
            if (command === 'workspace_write') {
              if (workspace.failWrites) throw new Error('Disk unavailable')
              const rel = args.relativePath!
              const content = args.content ?? ''
              await writeFile(location(rel), content, 'utf8')
              openDocs.set(rel, content)
              return { content, revision: revision(content) }
            }
            if (command === 'lsp_did_change') {
              const rel = args.relativePath!
              const content = args.content ?? ''
              openDocs.set(rel, content)
              const detected = detectLanguage(rel)
              if (detected) {
                const items = computeDiagnostics(content)
                diagnosticsByPath.set(rel, { languageId: detected.languageId, items })
                const lspStatus = syncServerCounts(detected.languageId, detected.command)
                await emit(page, 'lsp-diagnostics', {
                  workspaceRoot: workspace.root,
                  path: rel,
                  languageId: detected.languageId,
                  diagnostics: items,
                })
                await emit(page, 'lsp-status', lspStatus)
              }
              return null
            }
            if (command === 'lsp_hover') {
              const rel = args.relativePath!
              const offset = args.offset ?? 0
              const text = openDocs.get(rel) ?? (await readFile(location(rel), 'utf8'))
              const slice = text.slice(Math.max(0, offset - 12), Math.min(text.length, offset + 12))
              if (slice.includes('helper')) {
                const idx = text.indexOf('helper')
                return {
                  contents: 'fn helper() -> i32\nComputes the workspace answer.',
                  from: idx,
                  to: idx + 6,
                }
              }
              return null
            }
            if (command === 'lsp_goto_definition') {
              const rel = args.relativePath!
              const text = openDocs.get(rel) ?? (await readFile(location(rel), 'utf8'))
              const localDef = text.indexOf('fn helper')
              if (localDef !== -1) {
                return [
                  {
                    path: rel,
                    offset: localDef + 3,
                    endOffset: localDef + 9,
                    line: 0,
                    character: 3,
                  },
                ]
              }
              for (const [otherPath, otherText] of openDocs.entries()) {
                const idx = otherText.indexOf('pub fn helper')
                if (idx !== -1) {
                  return [
                    {
                      path: otherPath,
                      offset: idx + 7,
                      endOffset: idx + 13,
                      line: 0,
                      character: 7,
                    },
                  ]
                }
              }
              return []
            }
            if (command === 'lsp_get_state') {
              const diagRecord: Record<string, MockDiagnostic[]> = {}
              for (const [path, entry] of diagnosticsByPath.entries()) {
                if (entry.items.length > 0) {
                  diagRecord[path] = entry.items
                }
              }
              return {
                workspaceRoot: workspace.root,
                servers: Array.from(servers.values()),
                diagnosticsByPath: diagRecord,
              }
            }
            if (command === 'lsp_get_diagnostics_buffer') {
              const langFilter = args.languageId
              const title = langFilter ? `LSP Diagnostics (${langFilter})` : 'LSP Diagnostics'
              const lines = [`# ${title}`, `Workspace: ${workspace.root}`, '', '## Servers']
              const filteredServers = Array.from(servers.values()).filter(
                (s) => !langFilter || s.languageId === langFilter,
              )
              if (filteredServers.length === 0) {
                lines.push('No active language servers.')
              } else {
                for (const s of filteredServers) {
                  lines.push(
                    `- ${s.languageId} [${s.status}] (cmd: \`${s.command}\`, ${s.errorCount} error(s), ${s.warningCount} warning(s))`,
                  )
                }
              }
              lines.push('', '## Diagnostics')
              let count = 0
              for (const [path, entry] of diagnosticsByPath.entries()) {
                if (langFilter && entry.languageId !== langFilter) continue
                for (const d of entry.items) {
                  count++
                  lines.push(
                    `${path}:${d.line + 1}:${d.character + 1} [${d.severity.toUpperCase()}] [${d.code ?? ''}] (${d.source ?? 'lsp'}) ${d.message}`,
                  )
                }
              }
              if (count === 0) {
                lines.push('No diagnostics reported.')
              }
              return {
                title,
                languageId: langFilter ?? null,
                content: lines.join('\n'),
              }
            }
            if (command === 'workspace_frontend_ready') return null
            if (command === 'plugin:window|destroy') {
              workspace.destroyed = true
              return null
            }
            if (command === 'workspace_finish_quit') {
              workspace.quit = true
              return null
            }
            throw new Error(`Unexpected command: ${command}`)
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
            await rm(homeRoot, { recursive: true, force: true })
            for (const dir of extraDirs) {
              await rm(dir, { recursive: true, force: true })
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
