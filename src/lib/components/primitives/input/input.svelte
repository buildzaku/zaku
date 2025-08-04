<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";
  import type { WithElementRef } from "bits-ui";
  import { cn } from "$lib/utils/style.js";

  let {
    ref = $bindable(null),
    value = $bindable(),
    class: className,
    ...restProps
  }: WithElementRef<HTMLInputAttributes> = $props();
</script>

<input
  bind:this={ref}
  class={cn(
    "border-border text-small file:text-small placeholder:text-muted-foreground focus-visible:ring-ring flex h-6 w-full rounded-md border bg-transparent px-3 py-1 file:border-0 file:bg-transparent file:font-medium focus-visible:ring-1 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50",
    className,
  )}
  bind:value
  {...restProps}
/>
