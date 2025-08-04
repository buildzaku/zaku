<script lang="ts">
  import { Tabs as TabsPrimitive } from "bits-ui";
  import { cn } from "$lib/utils/style.js";

  let {
    ref = $bindable(null),
    class: className,
    value,
    ...restProps
  }: TabsPrimitive.ContentProps = $props();
</script>

<TabsPrimitive.Content
  bind:ref
  class={cn(
    "ring-offset-background focus-visible:ring-ring mt-2 focus-visible:ring-1 focus-visible:outline-none",
    className,
  )}
  {value}
  {...restProps}
/>
