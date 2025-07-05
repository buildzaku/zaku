<script lang="ts">
    import { Select as SelectPrimitive, type WithoutChildrenOrChild } from "bits-ui";
    import { cn } from "$lib/utils/style.js";
    import { ChevronUpIcon } from "@lucide/svelte";

    let {
        ref = $bindable(null),
        class: className,
        ...restProps
    }: WithoutChildrenOrChild<SelectPrimitive.ScrollDownButtonProps> = $props();
</script>

<SelectPrimitive.ScrollUpButton
    bind:ref
    class={cn("flex cursor-default items-center justify-center py-1", className)}
    {...restProps}
>
    <ChevronUpIcon class="size-4" />
</SelectPrimitive.ScrollUpButton>
