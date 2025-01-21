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
        "mt-2 ring-offset-background focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        className,
    )}
    {value}
    {...restProps}
/>
