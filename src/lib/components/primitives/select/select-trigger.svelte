<script lang="ts">
    import { Select as SelectPrimitive } from "bits-ui";
    import CaretDown from "svelte-radix/CaretDown.svelte";
    import { cn } from "$lib/utils/style.js";

    type $$Props = SelectPrimitive.TriggerProps & {
        withCaret?: boolean;
    };
    type $$Events = SelectPrimitive.TriggerEvents;

    let className: $$Props["class"] = undefined;

    export let withCaret: $$Props["withCaret"] = true;
    export { className as class };
</script>

<SelectPrimitive.Trigger
    class={cn(
        "flex h-6 w-full items-center justify-between whitespace-nowrap rounded-md border border-input bg-transparent px-2 py-1.5 text-small shadow-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 aria-[invalid]:border-destructive [&>span]:line-clamp-1 data-[placeholder]:[&>span]:text-muted-foreground",
        className,
    )}
    {...$$restProps}
>
    <slot />
    {#if withCaret}
        <div>
            <CaretDown class="h-4 w-4 text-muted-foreground opacity-50" />
        </div>
    {/if}
</SelectPrimitive.Trigger>
