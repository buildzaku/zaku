<script lang="ts">
    import * as ResizablePrimitive from "paneforge";
    import { cn } from "$lib/utils/style.js";

    type $$Props = ResizablePrimitive.PaneResizerProps & {
        withHandle?: boolean;
    };

    export let withHandle: $$Props["withHandle"] = false;
    export let el: $$Props["el"] = undefined;
    let className: $$Props["class"] = undefined;
    export { className as class };
</script>

<ResizablePrimitive.PaneResizer
    bind:el
    class={cn(
        "relative flex w-0 items-center justify-center border-[1px] border-transparent bg-muted after:absolute after:inset-y-0 after:left-1/2 after:w-1 after:-translate-x-1/2 hover:bg-primary/50 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1 data-[direction=vertical]:h-px data-[direction=vertical]:w-full data-[direction=horizontal]:data-[active]:border-primary/15 data-[direction=vertical]:data-[active]:border-primary/15 data-[active]:bg-primary/50 data-[direction=vertical]:after:left-0 data-[direction=vertical]:after:h-1 data-[direction=vertical]:after:w-full data-[direction=vertical]:after:-translate-y-1/2 data-[direction=vertical]:after:translate-x-0 data-[direction=horizontal]:hover:border-primary/15 data-[direction=vertical]:hover:border-primary/15 [&[data-direction=vertical]>div]:rotate-90",
        className,
    )}
>
    {#if withHandle}
        <div class="z-10">
            <div class="h-10 w-[2px] rounded bg-foreground/70" />
        </div>
    {/if}
</ResizablePrimitive.PaneResizer>
