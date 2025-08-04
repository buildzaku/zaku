<script lang="ts">
  import { Checkbox as CheckboxPrimitive, type WithoutChildrenOrChild } from "bits-ui";
  import { CheckIcon, MinusIcon } from "@lucide/svelte";
  import { cn } from "$lib/utils/style.js";

  let {
    ref = $bindable(null),
    class: className,
    checked = $bindable(false),
    indeterminate = $bindable(false),
    ...restProps
  }: WithoutChildrenOrChild<CheckboxPrimitive.RootProps> = $props();
</script>

<CheckboxPrimitive.Root
  class={cn(
    "peer border-primary focus-visible:ring-ring data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground box-content size-3.5 shrink-0 rounded-sm border focus-visible:ring-1 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50 data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50",
    className,
  )}
  bind:checked
  bind:ref
  bind:indeterminate
  {...restProps}
>
  {#snippet children({ checked, indeterminate })}
    <span class="flex size-3.5 items-center justify-center text-current">
      {#if indeterminate}
        <MinusIcon class="size-3" />
      {:else}
        <CheckIcon class={cn("size-3", !checked && "text-transparent")} />
      {/if}
    </span>
  {/snippet}
</CheckboxPrimitive.Root>
