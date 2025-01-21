<script lang="ts">
    import { Select as SelectPrimitive, type WithoutChild } from "bits-ui";
    import { CheckIcon } from "lucide-svelte";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        value,
        label,
        children: childrenProp,
        ...restProps
    }: WithoutChild<SelectPrimitive.ItemProps> = $props();
</script>

<SelectPrimitive.Item
    bind:ref
    {value}
    class={cn(
        "relative flex w-full cursor-default select-none items-center rounded-sm py-0.5 pl-2 pr-8 text-small outline-none data-[disabled]:pointer-events-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground data-[disabled]:opacity-50",
        className,
    )}
    {...restProps}
>
    {#snippet children({ selected, highlighted })}
        <span class="absolute right-2 flex size-3.5 items-center justify-center">
            {#if selected}
                <CheckIcon size={11} />
            {/if}
        </span>
        {#if childrenProp}
            {@render childrenProp({ selected, highlighted })}
        {:else}
            {label || value}
        {/if}
    {/snippet}
</SelectPrimitive.Item>
