<script lang="ts">
    import { Tabs as TabsPrimitive } from "bits-ui";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        ...restProps
    }: TabsPrimitive.ListProps = $props();
</script>

<TabsPrimitive.List
    bind:ref
    class={cn("text-muted-foreground inline-flex h-6 items-center justify-center p-1", className)}
    {...restProps}
/>
