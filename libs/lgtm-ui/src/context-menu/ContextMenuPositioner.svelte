<script lang="ts">
  import { Menu, useMenuContext } from '@ark-ui/svelte/menu'
  import { tick } from 'svelte'
  import { contextMenuPositionerBase } from './styles'
  import type { ContextMenuPositionerProps } from './types'

  // Props ---------------------------------------------------------------------
  let { children }: ContextMenuPositionerProps = $props()

  // State ---------------------------------------------------------------------
  const menu = useMenuContext()
  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    if (menu().open) void tick().then(() => requestAnimationFrame(() => menu().reposition()))
  })
</script>

<Menu.Positioner class={contextMenuPositionerBase}>
  {@render children?.()}
</Menu.Positioner>
