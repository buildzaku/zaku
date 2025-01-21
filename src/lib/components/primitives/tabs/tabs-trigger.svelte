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
        "inline-flex h-6 items-center justify-center whitespace-nowrap rounded-md border border-transparent px-1.5 text-small font-medium ring-offset-background transition-all focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 data-[state=active]:border-foreground/20 data-[state=active]:bg-muted data-[state=active]:text-foreground data-[state=active]:shadow",
        className,
    )}
    {value}
    {...restProps}
/>
