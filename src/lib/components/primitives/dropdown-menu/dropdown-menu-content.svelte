<script lang="ts">
    import { cn } from "$lib/utils/style.js";
    import { DropdownMenu as DropdownMenuPrimitive } from "bits-ui";

    let {
        ref = $bindable(null),
        class: className,
        sideOffset = 4,
        portalProps,
        ...restProps
    }: DropdownMenuPrimitive.ContentProps & {
        portalProps?: DropdownMenuPrimitive.PortalProps;
    } = $props();
</script>

<DropdownMenuPrimitive.Portal {...portalProps}>
    <DropdownMenuPrimitive.Content
        bind:ref
        {sideOffset}
        class={cn(
            "z-50 min-w-[8rem] rounded-md border bg-popover p-1 text-popover-foreground shadow-md focus:outline-none",
            className,
        )}
        {...restProps}
    />
</DropdownMenuPrimitive.Portal>
