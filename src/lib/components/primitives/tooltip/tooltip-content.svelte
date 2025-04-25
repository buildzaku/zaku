<script lang="ts">
    import { Tooltip as TooltipPrimitive } from "bits-ui";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        sideOffset = 4,
        ...restProps
    }: TooltipPrimitive.ContentProps = $props();
</script>

<TooltipPrimitive.Content
    bind:ref
    {sideOffset}
    class={cn(
        "border-ring bg-background text-foreground z-50 overflow-hidden rounded-md border px-2 py-0.5 text-[10px]",
        className,
    )}
    {...restProps}
/>
