<script lang="ts" module>
    import { type VariantProps, tv } from "tailwind-variants";
    export const badgeVariants = tv({
        base: "focus:ring-ring inline-flex select-none items-center rounded-md border px-1.5 text-[11px] font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2",
        variants: {
            variant: {
                default:
                    "bg-primary text-primary-foreground hover:bg-primary/80 border-transparent",
                secondary:
                    "bg-secondary text-secondary-foreground hover:bg-secondary/80 border-transparent",
                destructive:
                    "bg-destructive text-destructive-foreground hover:bg-destructive/80 border-transparent",
                outline: "text-foreground",
                success: "font-normal text-success bg-success-bg border-success-border",
                failure: "font-normal text-failure bg-failure-bg border-failure-border",
            },
        },
        defaultVariants: {
            variant: "default",
        },
    });

    export type BadgeVariant = VariantProps<typeof badgeVariants>["variant"];
</script>

<script lang="ts">
    import type { WithElementRef } from "bits-ui";
    import type { HTMLAnchorAttributes } from "svelte/elements";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        href,
        class: className,
        variant = "default",
        children,
        ...restProps
    }: WithElementRef<HTMLAnchorAttributes> & {
        variant?: BadgeVariant;
    } = $props();
</script>

<svelte:element
    this={href ? "a" : "span"}
    bind:this={ref}
    {href}
    class={cn(badgeVariants({ variant }), className)}
    {...restProps}
>
    {@render children?.()}
</svelte:element>
