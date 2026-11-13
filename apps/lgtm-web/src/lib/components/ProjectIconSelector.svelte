<script lang="ts">
  import { ToggleGroup, ToggleGroupItem } from '@cloudvoyant/helical-svelte'
  import { PROJECT_ICONS, resolveColorHex } from '../constants'

  // Props ---------------------------------------------------------------------
  interface ProjectIconSelectorProps {
    value: string
    color?: string | null
    onchange?: (iconId: string) => void
  }
  let { value = 'box', color = null, onchange }: ProjectIconSelectorProps = $props()
</script>

<ToggleGroup
  value={[value]}
  multiple={false}
  aria-label="Project icon"
  class="grid grid-cols-8 gap-1"
  onValueChange={({ value: next }) => {
    if (next[0]) {
      onchange?.(next[0])
    }
  }}
>
  {#each PROJECT_ICONS as item (item.id)}
    {@const IconComp = item.component}
    {@const active = value === item.id}
    {@const previewHex = resolveColorHex(color)}
    <ToggleGroupItem
      value={item.id}
      variant="outline"
      size="sm"
      aria-label="Icon {item.label}"
      title={item.label}
      style={active && previewHex ? `color: ${previewHex}` : undefined}
      class="size-7 p-0"
    >
      <IconComp size={14} strokeWidth={1.6} />
    </ToggleGroupItem>
  {/each}
</ToggleGroup>
