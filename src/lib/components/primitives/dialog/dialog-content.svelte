<script lang="ts">
  import type { Snippet } from "svelte";
  import { Dialog as DialogPrimitive, type WithoutChildrenOrChild } from "bits-ui";
  import { XIcon } from "@lucide/svelte";

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
      "bg-background fixed top-[50%] left-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border p-6 sm:rounded-lg",
      className,
    )}
    {...restProps}
  >
    {@render children?.()}
    <DialogPrimitive.Close
      class="ring-offset-background focus:ring-ring data-[state=open]:text-muted-foreground absolute top-4 right-4 cursor-pointer rounded-sm opacity-70 transition-opacity hover:opacity-100 focus:ring-1 focus:ring-offset-0 focus:outline-none disabled:pointer-events-none"
    >
      <XIcon class="size-4" />
      <span class="sr-only">Close</span>
    </DialogPrimitive.Close>
  </DialogPrimitive.Content>
</Dialog.Portal>
