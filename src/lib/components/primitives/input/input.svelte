<script lang="ts">
    import type { HTMLInputAttributes } from "svelte/elements";
    import type { WithElementRef } from "bits-ui";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        value = $bindable(),
        class: className,
        ...restProps
    }: WithElementRef<HTMLInputAttributes> = $props();
</script>

<input
    bind:this={ref}
    class={cn(
        "flex h-6 w-full rounded-md border border-input bg-transparent px-3 py-1 text-small shadow-sm file:border-0 file:bg-transparent file:text-small file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        className,
    )}
    bind:value
    {...restProps}
/>
