<script lang="ts">
  import { Tabs } from '@ark-ui/svelte/tabs'
  import { cn } from '@cloudvoyant/helical-ui'
  import { X } from 'lucide-svelte'
  import { tick } from 'svelte'
  import {
    tabsCloseButtonBase,
    tabsDirtyIndicatorBase,
    tabsItemContainerVariants,
    tabsListVariants,
    tabsRootBase,
    tabsTriggerVariants,
  } from './styles'
  import type { TabsProps } from './types'

  // Props ---------------------------------------------------------------------
  let {
    items,
    value,
    ariaLabel = 'Open editors',
    size = 'sm',
    class: className = '',
    onchange,
    onclose,
  }: TabsProps = $props()

  // State ---------------------------------------------------------------------
  let listRef = $state<HTMLDivElement | null>(null)

  // Derived -------------------------------------------------------------------
  const rootClasses = $derived(cn(tabsRootBase, className))
  const listClasses = $derived(cn(tabsListVariants({ size })))
  const triggerClasses = $derived(cn(tabsTriggerVariants({ size })))

  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    const activeValue = value
    if (!activeValue || !listRef) return
    void tick().then(() => {
      const target = listRef?.querySelector<HTMLElement>(
        `[data-tab-id=${JSON.stringify(activeValue)}]`,
      )
      target?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
    })
  })
</script>

<Tabs.Root
  value={value ?? undefined}
  onValueChange={({ value: next }) => onchange(next)}
  class={rootClasses}
>
  <Tabs.List bind:ref={listRef} aria-label={ariaLabel} class={listClasses}>
    {#each items as item (item.value)}
      <div
        class={tabsItemContainerVariants({ size })}
        data-selected={value === item.value ? '' : undefined}
        data-dirty={item.dirty ? 'true' : undefined}
        data-tab-id={item.value}
      >
        <Tabs.Trigger value={item.value} class={triggerClasses}>
          <span>{item.label}</span>
          {#if item.dirty}
            <span
              class={tabsDirtyIndicatorBase}
              aria-label="Unsaved changes"
              title="Unsaved changes"
            ></span>
          {/if}
        </Tabs.Trigger>
        {#if item.closable !== false && onclose}
          <button
            type="button"
            class={tabsCloseButtonBase}
            aria-label={`Close ${item.label}`}
            onclick={(event) => {
              event.stopPropagation()
              onclose(item.value)
            }}
          >
            <X size={10} strokeWidth={1.75} />
          </button>
        {/if}
      </div>
    {/each}
  </Tabs.List>
</Tabs.Root>
