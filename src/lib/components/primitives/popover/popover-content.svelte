<script lang="ts">
    import { cn } from "$lib/utils/style.js";
    import { Popover as PopoverPrimitive } from "bits-ui";

    let {
        ref = $bindable(null),
        class: className,
        align = "center",
        sideOffset = 4,
        portalProps,
        ...restProps
    }: PopoverPrimitive.ContentProps & {
        portalProps?: PopoverPrimitive.PortalProps;
    } = $props();
</script>

<PopoverPrimitive.Portal {...portalProps}>
    <PopoverPrimitive.Content
        bind:ref
        {align}
        {sideOffset}
        class={cn(
            "bg-popover text-popover-foreground z-50 w-72 rounded-md border p-4 shadow-md outline-none",
            className,
        )}
        {...restProps}
    />
</PopoverPrimitive.Portal>
