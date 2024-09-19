<script lang="ts">
    import { Select as SelectPrimitive } from "bits-ui";
    import { scale } from "svelte/transition";
    import { cn } from "$lib/utils/style.js";

    type $$Props = SelectPrimitive.ContentProps;

    let className: $$Props["class"] = undefined;
    export let sideOffset: $$Props["sideOffset"] = 4;
    export { className as class };
</script>

<SelectPrimitive.Content
    {sideOffset}
    class={cn(
        "relative z-50 min-w-[8rem] overflow-hidden rounded-md border bg-popover text-popover-foreground shadow-md focus:outline-none",
        className,
    )}
    {...$$restProps}
>
    <div class="w-full p-1">
        <slot />
    </div>
</SelectPrimitive.Content>
