<script lang="ts">
    import { Select as SelectPrimitive, type WithoutChild } from "bits-ui";
    import ChevronDown from "lucide-svelte/icons/chevron-down";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        children,
        ...restProps
    }: WithoutChild<SelectPrimitive.TriggerProps> = $props();
</script>

<SelectPrimitive.Trigger
    bind:ref
    class={cn(
        "border-input text-small ring-offset-background focus:ring-ring data-[placeholder]:text-muted-foreground flex h-6 w-full items-center justify-between rounded-md border bg-transparent px-2 py-1.5 whitespace-nowrap shadow-sm focus:ring-1 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50 [&>span]:line-clamp-1",
        className,
    )}
    {...restProps}
>
    {@render children?.()}
    <ChevronDown class="size-4 opacity-50" />
</SelectPrimitive.Trigger>
