<script lang="ts">
    import { DropdownMenu as DropdownMenuPrimitive, type WithoutChildrenOrChild } from "bits-ui";
    import { CheckIcon, MinusIcon } from "lucide-svelte";
    import { cn } from "$lib/utils/style.js";
    import type { Snippet } from "svelte";

    let {
        ref = $bindable(null),
        class: className,
        children: childrenProp,
        checked = $bindable(false),
        indeterminate = $bindable(false),
        ...restProps
    }: WithoutChildrenOrChild<DropdownMenuPrimitive.CheckboxItemProps> & {
        children?: Snippet;
    } = $props();
</script>

<DropdownMenuPrimitive.CheckboxItem
    bind:ref
    bind:checked
    bind:indeterminate
    class={cn(
        "data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground relative flex cursor-default items-center rounded-sm py-1.5 pr-2 pl-8 text-sm outline-none select-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
        className,
    )}
    {...restProps}
>
    {#snippet children({ checked, indeterminate })}
        <span class="absolute left-2 flex size-3.5 items-center justify-center">
            {#if indeterminate}
                <MinusIcon class="size-4" />
            {:else}
                <CheckIcon class={cn("size-4", !checked && "text-transparent")} />
            {/if}
        </span>
        {@render childrenProp?.()}
    {/snippet}
</DropdownMenuPrimitive.CheckboxItem>
