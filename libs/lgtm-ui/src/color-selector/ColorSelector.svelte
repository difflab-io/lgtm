<script lang="ts">
  import { ToggleGroup } from '@ark-ui/svelte/toggle-group'
  import { cn } from '@cloudvoyant/helical-ui'
  import { colorSelectorItemVariants, colorSelectorRootBase } from './styles'
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
      class={cn(itemClasses, !item.color && 'bg-muted-foreground/70')}
      style={item.color ? `background-color: ${item.color}` : undefined}
    />
  {/each}
</ToggleGroup.Root>
