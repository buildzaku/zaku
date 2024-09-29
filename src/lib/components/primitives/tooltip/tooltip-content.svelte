<script lang="ts">
    import { Tooltip as TooltipPrimitive } from "bits-ui";
    import { cn } from "$lib/utils/style.js";

    type $$Props = TooltipPrimitive.ContentProps;

    let className: $$Props["class"] = undefined;
    export let sideOffset: $$Props["sideOffset"] = 4;
    export { className as class };
</script>

<TooltipPrimitive.Content
    {sideOffset}
    class={cn(
        "z-50 overflow-hidden rounded-md border border-ring bg-background px-2 py-0.5 text-[10px] text-foreground",
        className,
    )}
    {...$$restProps}
>
    <slot />
</TooltipPrimitive.Content>
