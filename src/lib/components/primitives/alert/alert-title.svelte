<script lang="ts">
    import type { HTMLAttributes } from "svelte/elements";
    import type { WithElementRef } from "bits-ui";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        level = 5,
        children,
        ...restProps
    }: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
        level?: 1 | 2 | 3 | 4 | 5 | 6;
    } = $props();
</script>

<div
    bind:this={ref}
    aria-level={level}
    class={cn("mb-1 leading-none font-medium tracking-tight", className)}
    {...restProps}
>
    {@render children?.()}
</div>
