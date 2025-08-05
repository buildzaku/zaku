<script lang="ts">
  import { Tabs as TabsPrimitive } from "bits-ui";
  import { cn } from "$lib/utils/style.js";

  let {
    ref = $bindable(null),
    class: className,
    value,
    ...restProps
  }: TabsPrimitive.TriggerProps = $props();
</script>

<TabsPrimitive.Trigger
  bind:ref
  class={cn(
    "text-small ring-offset-background focus-visible:ring-ring data-[state=active]:border-foreground/20 data-[state=active]:bg-muted/75 data-[state=active]:text-foreground inline-flex h-6 cursor-pointer items-center justify-center rounded-md border border-transparent px-1.5 font-medium whitespace-nowrap transition-all focus-visible:ring-1 focus-visible:outline-none disabled:pointer-events-none disabled:opacity-50",
    className,
  )}
  {value}
  {...restProps}
/>
