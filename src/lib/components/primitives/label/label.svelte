<script lang="ts">
    import { Label as LabelPrimitive } from "bits-ui";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        ...restProps
    }: LabelPrimitive.RootProps = $props();
</script>

<LabelPrimitive.Root
    bind:ref
    class={cn(
        "text-sm leading-none font-medium peer-disabled:cursor-not-allowed peer-disabled:opacity-70",
        className,
    )}
    {...restProps}
/>
