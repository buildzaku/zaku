<script lang="ts">
    import { Select as SelectPrimitive } from "bits-ui";
    import { cn } from "$lib/utils/style.js";

    type $$Props = SelectPrimitive.LabelProps;

    let className: $$Props["class"] = undefined;
    export { className as class };
</script>

<SelectPrimitive.Label
    class={cn("px-2 py-1.5 text-small font-semibold", className)}
    {...$$restProps}
>
    <slot />
</SelectPrimitive.Label>
