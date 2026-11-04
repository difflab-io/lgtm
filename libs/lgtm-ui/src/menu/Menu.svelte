<script lang="ts">
  import { Menu } from '@ark-ui/svelte/menu'
  import { Portal } from '@cloudvoyant/helical-svelte'
  import { cn } from '@cloudvoyant/helical-ui'
  import { ChevronRight } from 'lucide-svelte'
  import MenuPositioner from './MenuPositioner.svelte'
  import {
    menuContentBase,
    menuIndicatorVariants,
    menuItemDetailBase,
    menuItemLabelBase,
    menuItemMainBase,
    menuItemVariants,
    menuSeparatorBase,
    menuTriggerBase,
  } from './styles'
  import type { MenuItem, MenuProps } from './types'

  // Props ---------------------------------------------------------------------
  let {
    trigger,
    items,
    ariaLabel,
    triggerAriaLabel,
    triggerTitle,
    triggerClass = '',
    placement = 'top-end',
    size = 'sm',
    class: className = '',
    onselect,
    onopenchange,
  }: MenuProps = $props()

  // Derived -------------------------------------------------------------------
  const triggerClasses = $derived(cn(menuTriggerBase, triggerClass))
  const contentClasses = $derived(cn(menuContentBase, className))
</script>

{#snippet renderItemBody(item: MenuItem, hasSubmenu: boolean)}
  {#if item.render}
    {@render item.render(item)}
  {:else}
    <span class={menuItemMainBase}>
      {#if item.indicator}
        <span
          class={menuIndicatorVariants({ tone: item.indicator })}
          data-indicator={item.indicator}
          title={item.indicatorLabel ?? item.indicator}
        ></span>
      {/if}
      <span class={menuItemLabelBase}>{item.label}</span>
    </span>
    {#if item.detail}
      <span class={menuItemDetailBase}>{item.detail}</span>
    {:else if item.shortcut}
      <span class={menuItemDetailBase}>{item.shortcut}</span>
    {/if}
    {#if hasSubmenu}
      <ChevronRight size={12} strokeWidth={1.75} class="shrink-0 text-muted-foreground" />
    {/if}
  {/if}
{/snippet}

<Menu.Root
  positioning={{ placement }}
  onSelect={({ value }) => onselect(value)}
  onOpenChange={({ open }) => onopenchange?.(open)}
>
  <Menu.Trigger class={triggerClasses} aria-label={triggerAriaLabel} title={triggerTitle}>
    {@render trigger({})}
  </Menu.Trigger>
  <Portal>
    <MenuPositioner>
      <Menu.Content>
        {#snippet asChild(props)}
          <div
            {...props()}
            aria-labelledby={ariaLabel
              ? undefined
              : (props()['aria-labelledby'] as string | undefined)}
            aria-label={ariaLabel}
            class={cn(props().class as string | undefined, contentClasses)}
          >
            {#each items as item (item.value)}
              {#if item.separatorBefore}
                <Menu.Separator class={menuSeparatorBase} />
              {/if}
              {#if item.items && item.items.length > 0}
                <Menu.Root
                  positioning={{ placement: 'left-start' }}
                  onSelect={({ value }) => onselect(value)}
                >
                  <Menu.TriggerItem
                    class={menuItemVariants({ size, destructive: item.destructive })}
                    data-value={item.value}
                  >
                    {@render renderItemBody(item, true)}
                  </Menu.TriggerItem>
                  <Portal>
                    <MenuPositioner>
                      <Menu.Content>
                        {#snippet asChild(subProps)}
                          <div
                            {...subProps()}
                            aria-labelledby={undefined}
                            aria-label={`${item.label} actions`}
                            class={cn(subProps().class as string | undefined, contentClasses)}
                          >
                            {#each item.items as subItem (subItem.value)}
                              {#if subItem.separatorBefore}
                                <Menu.Separator class={menuSeparatorBase} />
                              {/if}
                              <Menu.Item
                                value={subItem.value}
                                disabled={subItem.disabled}
                                class={menuItemVariants({ size, destructive: subItem.destructive })}
                                data-destructive={subItem.destructive || undefined}
                              >
                                {@render renderItemBody(subItem, false)}
                              </Menu.Item>
                            {/each}
                          </div>
                        {/snippet}
                      </Menu.Content>
                    </MenuPositioner>
                  </Portal>
                </Menu.Root>
              {:else}
                <Menu.Item
                  value={item.value}
                  disabled={item.disabled}
                  class={menuItemVariants({ size, destructive: item.destructive })}
                  data-destructive={item.destructive || undefined}
                >
                  {@render renderItemBody(item, false)}
                </Menu.Item>
              {/if}
            {/each}
          </div>
        {/snippet}
      </Menu.Content>
    </MenuPositioner>
  </Portal>
</Menu.Root>
