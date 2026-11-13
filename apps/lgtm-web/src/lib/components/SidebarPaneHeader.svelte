<script lang="ts">
  import type { Snippet } from 'svelte'
  import { Row, SidebarHeader } from '@cloudvoyant/helical-svelte'

  // Props ---------------------------------------------------------------------
  interface SidebarPaneHeaderProps {
    title: string
    class?: string
    actions?: Snippet
  }
  let { title, class: className = '', actions }: SidebarPaneHeaderProps = $props()
</script>

<SidebarHeader
  class={[
    'sticky top-0 z-1 shrink-0 items-stretch gap-1 bg-sidebar px-3.5 pt-3.5 pb-2.5 text-[11px] font-semibold text-muted-foreground',
    className,
  ].join(' ')}
>
  <Row class="items-center justify-between gap-1.5">
    <span>{title}</span>
    {#if actions}
      <Row class="items-center gap-0.5">
        {@render actions()}
      </Row>
    {/if}
  </Row>
</SidebarHeader>
