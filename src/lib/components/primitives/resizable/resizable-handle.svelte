<script lang="ts">
    import * as ResizablePrimitive from "paneforge";
    import type { WithoutChildrenOrChild } from "bits-ui";

    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        withHandle = false,
        ...restProps
    }: WithoutChildrenOrChild<ResizablePrimitive.PaneResizerProps> & {
        withHandle?: boolean;
    } = $props();
</script>

<ResizablePrimitive.PaneResizer
    bind:ref
    class={cn(
        "bg-muted hover:bg-primary/50 focus-visible:ring-ring data-[direction=horizontal]:data-[active]:border-primary/15 data-[direction=vertical]:data-[active]:border-primary/15 data-[active]:bg-primary/50 data-[direction=horizontal]:hover:border-primary/15 data-[direction=vertical]:hover:border-primary/15 relative flex w-0 items-center justify-center border-[1px] border-transparent after:absolute after:inset-y-0 after:left-1/2 after:w-1 after:-translate-x-1/2 focus-visible:ring-1 focus-visible:ring-offset-1 focus-visible:outline-none data-[direction=vertical]:h-px data-[direction=vertical]:w-full data-[direction=vertical]:after:left-0 data-[direction=vertical]:after:h-1 data-[direction=vertical]:after:w-full data-[direction=vertical]:after:translate-x-0 data-[direction=vertical]:after:-translate-y-1/2 [&[data-direction=vertical]>div]:rotate-90",
        className,
    )}
    {...restProps}
>
    {#if withHandle}
        <div class="z-10">
            <div class="bg-foreground/70 h-10 w-[2px] rounded"></div>
        </div>
    {/if}
</ResizablePrimitive.PaneResizer>
