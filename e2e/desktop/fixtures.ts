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
  emit: (event: string) => Promise<void>
  createExternalDir: () => Promise<string>
}

interface CommandArgs {
  path?: string
  name?: string
  parentPath?: string
  relativePath?: string
}

const revision = (content: string) => createHash('sha256').update(content).digest('hex')

async function emit(page: Page, event: string) {
  await page.evaluate(async (name) => {
    await (
      window as unknown as { workspaceTestEmit: (event: string) => Promise<void> }
    ).workspaceTestEmit(name)
  }, event)
}

// Fixtures --------------------------------------------------------------------
// The browser runs the production UI. Only the native IPC transport is substituted;
// workspace contents and persistence assertions use real, test-owned directories.
export const test = base.extend<{ workspace: Workspace; initialWorkspace: 'empty' | 'existing' }>({
  initialWorkspace: ['existing', { option: true }],
  workspace: [
    async ({ page, initialWorkspace }, use, testInfo) => {
      const initialRoot = await mkdtemp(join(tmpdir(), 'lgtm-ui-'))
      const extraDirs: string[] = []
      const workspace: Workspace = {
        root: initialRoot,
        pickedFolder: null,
        failLists: false,
        failWrites: false,
        destroyed: false,
        quit: false,
        emit: (event) => emit(page, event),
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
              }
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
                const exists = await stat(location(path)).then(
                  () => true,
                  () => false,
                )
                if (exists) throw new Error('A file or folder with this name already exists.')
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
              return null
            }
            if (command === 'workspace_read') {
              const content = await readFile(location(args.relativePath!), 'utf8')
              return { content, revision: revision(content) }
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
            await rm(initialRoot, { recursive: true, force: true })
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
