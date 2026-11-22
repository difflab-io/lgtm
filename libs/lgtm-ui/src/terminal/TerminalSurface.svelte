<script lang="ts">
  import { cn } from '@cloudvoyant/helical-ui'
  import { onMount } from 'svelte'
  import { terminalHostBase, terminalSurfaceBase } from './styles'
  import {
    TERMINAL_FONT_FAMILY,
    getTerminalTheme,
    observeTerminalTheme,
    toWtermThemeColors,
  } from './theme'
  import type { TerminalSurfaceHandle, TerminalSurfaceProps } from './types'

  let {
    sessionId,
    theme,
    class: className = '',
    onInput,
    onResize,
    onReady,
  }: TerminalSurfaceProps = $props()

  let hostEl = $state<HTMLDivElement>()
  let ready = $state(false)
  let fallbackLog = $state('')

  const encoder = new TextEncoder()
  const decoder = new TextDecoder()

  let wtermInstance: {
    cols: number
    rows: number
    write: (data: string | Uint8Array) => void
    clear?: () => void
    fit: () => void
    focus?: () => void
    setThemeColors: (colors: ReturnType<typeof toWtermThemeColors>) => void
    destroy: () => void
  } | null = null

  let pendingWrites: Uint8Array[] = []

  function focusSurface() {
    if (wtermInstance?.focus) {
      wtermInstance.focus()
      return
    }
    const textarea = hostEl?.querySelector('textarea')
    if (textarea instanceof HTMLTextAreaElement) {
      textarea.focus()
    }
  }

  function createHandle(): TerminalSurfaceHandle {
    return {
      write(bytes: Uint8Array) {
        if (wtermInstance) {
          wtermInstance.write(bytes)
        } else {
          pendingWrites.push(bytes)
        }
        fallbackLog = (fallbackLog + decoder.decode(bytes, { stream: true })).slice(-8192)
      },
      clear() {
        wtermInstance?.clear?.()
        fallbackLog = ''
      },
      focus() {
        focusSurface()
      },
      fit() {
        try {
          wtermInstance?.fit()
        } catch {
          // ignore fit errors while transitioning width
        }
      },
    }
  }

  onMount(() => {
    let disposed = false
    const handle = createHandle()
    onReady?.(handle)

    void (async () => {
      try {
        const [{ WTerm }, { GhosttyCore }] = await Promise.all([
          import('@wterm/dom'),
          import('@wterm/ghostty'),
        ])
        const core = await GhosttyCore.load({ scrollbackLines: 2000 })
        if (disposed || !hostEl) {
          core.dispose?.()
          return
        }

        const term = new WTerm(hostEl, {
          core,
          renderer: 'webgl',
          autoResize: true,
          scrollbackLines: 2000,
          onData: (data: string) => {
            onInput(encoder.encode(data))
          },
          onResize: (nextCols: number, nextRows: number) => {
            onResize(Math.max(20, nextCols), Math.max(4, nextRows))
          },
        })
        await term.init()
        if (disposed) {
          term.destroy()
          return
        }
        term.setThemeColors(toWtermThemeColors(theme ?? getTerminalTheme()))
        term.fit()
        wtermInstance = term as unknown as typeof wtermInstance
        ready = true

        for (const chunk of pendingWrites) {
          wtermInstance?.write(chunk)
        }
        pendingWrites = []
        onResize(Math.max(20, term.cols), Math.max(4, term.rows))
      } catch {
        ready = false
      }
    })()

    const stopThemeObserver = observeTerminalTheme((nextTheme) => {
      if (!theme && wtermInstance) {
        wtermInstance.setThemeColors(toWtermThemeColors(nextTheme))
      }
    })

    let resizeObserver: ResizeObserver | null = null
    if (typeof ResizeObserver !== 'undefined' && hostEl) {
      resizeObserver = new ResizeObserver(() => {
        if (!disposed) {
          handle.fit()
        }
      })
      resizeObserver.observe(hostEl)
    }

    return () => {
      disposed = true
      pendingWrites = []
      stopThemeObserver()
      resizeObserver?.disconnect()
      try {
        wtermInstance?.destroy()
      } catch {
        // ignore WebGL/WASM teardown errors
      }
      wtermInstance = null
    }
  })
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class={cn(terminalSurfaceBase, className)}
  style:font-family={TERMINAL_FONT_FAMILY}
  role="region"
  aria-label="Terminal surface"
  data-testid="terminal-surface"
  data-terminal-session-id={sessionId}
  data-terminal-ready={ready ? 'true' : 'false'}
  onclick={() => focusSurface()}
>
  <div bind:this={hostEl} class={terminalHostBase} style:font-family={TERMINAL_FONT_FAMILY}></div>
  {#if !ready && fallbackLog}
    <pre
      data-testid="terminal-fallback-output"
      class="pointer-events-none absolute inset-0 overflow-y-auto px-3 py-2 text-[11px] whitespace-pre-wrap text-foreground">{fallbackLog}</pre>
  {/if}
</div>
