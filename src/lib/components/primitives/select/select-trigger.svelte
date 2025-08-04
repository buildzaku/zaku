<script lang="ts">
  import { Select as SelectPrimitive, type WithoutChild } from "bits-ui";
  import { cn } from "$lib/utils/style.js";
  import { ChevronDownIcon } from "@lucide/svelte";

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithoutChild<SelectPrimitive.TriggerProps> = $props();
</script>

<SelectPrimitive.Trigger
  bind:ref
  class={cn(
    "text-small ring-offset-background focus:ring-ring data-[placeholder]:text-muted-foreground flex h-6 w-full items-center justify-between rounded-md border border bg-transparent px-2 py-1.5 whitespace-nowrap focus:ring-1 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50 [&>span]:line-clamp-1",
    className,
  )}
  {...restProps}
>
  {@render children?.()}
  <ChevronDownIcon class="size-4 opacity-50" />
</SelectPrimitive.Trigger>
