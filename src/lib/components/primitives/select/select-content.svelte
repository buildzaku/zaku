<script lang="ts">
    import { Select as SelectPrimitive, type WithoutChild } from "bits-ui";
    import * as Select from "./index.js";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        sideOffset = 4,
        portalProps,
        children,
        ...restProps
    }: WithoutChild<SelectPrimitive.ContentProps> & {
        portalProps?: SelectPrimitive.PortalProps;
    } = $props();
</script>

<SelectPrimitive.Portal {...portalProps}>
    <SelectPrimitive.Content
        bind:ref
        {sideOffset}
        class={cn(
            "relative z-50 overflow-hidden rounded-md border bg-popover text-popover-foreground shadow-md focus:outline-none",
            className,
        )}
        {...restProps}
    >
        <Select.ScrollUpButton />
        <SelectPrimitive.Viewport class={cn("h-[var(--bits-select-anchor-height)] w-full p-1")}>
            {@render children?.()}
        </SelectPrimitive.Viewport>
        <Select.ScrollDownButton />
    </SelectPrimitive.Content>
</SelectPrimitive.Portal>
