<script lang="ts">
  import { ToggleGroup } from '@ark-ui/svelte/toggle-group'
  import { cn } from '@cloudvoyant/helical-ui'
  import { Check } from 'lucide-svelte'
  import {
    colorSelectorIndicatorBase,
    colorSelectorItemVariants,
    colorSelectorRootBase,
  } from './styles'
  import type { ColorSelectorProps } from './types'

  // Props ---------------------------------------------------------------------
  let {
    items,
    value,
    ariaLabel = 'Project color',
    size = 'md',
    class: className = '',
    onchange,
  }: ColorSelectorProps = $props()

  // Derived -------------------------------------------------------------------
  const rootClasses = $derived(cn(colorSelectorRootBase, className))
  const itemClasses = $derived(colorSelectorItemVariants({ size }))
</script>

<ToggleGroup.Root
  value={[value]}
  multiple={false}
  aria-label={ariaLabel}
  class={rootClasses}
  onValueChange={({ value: next }) => {
    const selected = next[0]
    if (selected) onchange?.(selected)
  }}
>
  {#each items as item (item.value)}
    <ToggleGroup.Item
      value={item.value}
      aria-label={`Color ${item.label}`}
      title={item.label}
      class={cn(itemClasses, !item.color && 'bg-muted-foreground')}
      style={item.color ? `background-color: ${item.color}` : undefined}
    >
      {#if value === item.value}
        <Check class={colorSelectorIndicatorBase} strokeWidth={2.75} />
      {/if}
    </ToggleGroup.Item>
  {/each}
</ToggleGroup.Root>
