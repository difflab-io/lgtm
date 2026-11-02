import { mount } from 'svelte'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { emit } from '@tauri-apps/api/event'
import Shell from '../../apps/lgtm-web/src/lib/components/Shell.svelte'
import '../../apps/lgtm-web/src/app.css'

// This page is included only in the isolated test build, never included in the application build.
const testWindow = window as unknown as {
  workspaceTestInvoke: (command: string, args: unknown) => Promise<unknown>
  workspaceTestEmit: (event: string) => Promise<void>
  isTauri: boolean
}
mockWindows('main')
mockIPC((command, args) => testWindow.workspaceTestInvoke(command, args), {
  shouldMockEvents: true,
})
testWindow.isTauri = true
testWindow.workspaceTestEmit = (event) => emit(event)
mount(Shell, { target: document.querySelector('#app')! })
