<script lang="ts">
  import { Ark } from '@ark-ui/svelte/factory'
  import { cn } from '@cloudvoyant/helical-ui'
  import { Monitor, Moon, Sun } from 'lucide-svelte'
  import { themeToggleVariants } from './styles'
  import type { ThemePreference, ThemeToggleProps } from './types'

  const THEME_ORDER: readonly ThemePreference[] = ['system', 'light', 'dark']

  const THEME_LABELS: Record<ThemePreference, string> = {
    system: 'Theme: system',
    light: 'Theme: light',
    dark: 'Theme: dark',
  }

  // Props ---------------------------------------------------------------------
  let {
    value,
    size = 'sm',
    variant = 'ghost',
    class: className = '',
    onchange,
  }: ThemeToggleProps = $props()

  // Derived -------------------------------------------------------------------
  const classes = $derived(cn(themeToggleVariants({ size, variant }), className))
  const iconSize = $derived(size === 'sm' ? 14 : 16)
  const label = $derived(THEME_LABELS[value])

  function cycleTheme() {
    const currentIndex = THEME_ORDER.indexOf(value)
    const nextValue = THEME_ORDER[(currentIndex + 1) % THEME_ORDER.length] ?? 'system'
    onchange(nextValue)
  }
</script>

<Ark
  as="button"
  type="button"
  class={classes}
  aria-label={label}
  title={label}
  data-slot="theme-toggle"
  data-theme={value}
  onclick={cycleTheme}
>
  {#if value === 'light'}
    <Sun size={iconSize} strokeWidth={1.5} />
  {:else if value === 'dark'}
    <Moon size={iconSize} strokeWidth={1.5} />
  {:else}
    <Monitor size={iconSize} strokeWidth={1.5} />
  {/if}
</Ark>
