<script lang="ts">
    import type { Snippet } from "svelte";
    import { Dialog as DialogPrimitive, type WithoutChildrenOrChild } from "bits-ui";
    import { XIcon } from "lucide-svelte";

    import * as Dialog from "./index.js";
    import { cn } from "$lib/utils/style.js";

    let {
        ref = $bindable(null),
        class: className,
        portalProps,
        children,
        ...restProps
    }: WithoutChildrenOrChild<DialogPrimitive.ContentProps> & {
        portalProps?: DialogPrimitive.PortalProps;
        children: Snippet;
    } = $props();
</script>

<Dialog.Portal {...portalProps}>
    <Dialog.Overlay />
    <DialogPrimitive.Content
        bind:ref
        class={cn(
            "fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border bg-background p-6 shadow-lg sm:rounded-lg",
            className,
        )}
        {...restProps}
    >
        {@render children?.()}
        <DialogPrimitive.Close
            class="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground"
        >
            <XIcon class="size-4" />
            <span class="sr-only">Close</span>
        </DialogPrimitive.Close>
    </DialogPrimitive.Content>
</Dialog.Portal>
