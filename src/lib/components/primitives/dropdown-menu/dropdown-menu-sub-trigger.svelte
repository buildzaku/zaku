<script lang="ts">
    import { DropdownMenu as DropdownMenuPrimitive, type WithoutChild } from "bits-ui";
    import ChevronRight from "lucide-svelte/icons/chevron-right";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        inset,
        children,
        ...restProps
    }: WithoutChild<DropdownMenuPrimitive.SubTriggerProps> & {
        inset?: boolean;
    } = $props();
</script>

<DropdownMenuPrimitive.SubTrigger
    bind:ref
    class={cn(
        "data-[highlighted]:bg-accent data-[state=open]:bg-accent flex cursor-default items-center rounded-sm px-2 py-1.5 text-sm outline-none select-none [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
        inset && "pl-8",
        className,
    )}
    {...restProps}
>
    {@render children?.()}
    <ChevronRight class="ml-auto size-4" />
</DropdownMenuPrimitive.SubTrigger>
