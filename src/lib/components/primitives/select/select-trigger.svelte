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
        "flex h-6 w-full items-center justify-between whitespace-nowrap rounded-md border border-input bg-transparent px-2 py-1.5 text-small shadow-sm ring-offset-background focus:outline-none focus:ring-1 focus:ring-ring disabled:cursor-not-allowed disabled:opacity-50 data-[placeholder]:text-muted-foreground [&>span]:line-clamp-1",
        className,
    )}
    {...restProps}
>
    {@render children?.()}
    <ChevronDown class="size-4 opacity-50" />
</SelectPrimitive.Trigger>
