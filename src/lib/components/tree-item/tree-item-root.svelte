<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "lucide-svelte";

    import { handleDragOver, handleDrop, handleDragEnd, isDropAllowed } from ".";
    import type { Collection } from "$lib/models";
    import { currentDragPayload, currentDropTargetPath } from "$lib/store";
    import { cn } from "$lib/utils/style";
    import { Button } from "$lib/components/primitives/button";
    import { FilePlusIcon, FolderPlusIcon } from "$lib/components/icons";
    import { Tooltip, TooltipTrigger, TooltipContent } from "$lib/components/primitives/tooltip";

    export let currentPath: string;
    export let root: Collection;

    let propsClass = $$props["class"];
    let shouldHighlight = isDropAllowed(currentPath);

    $: {
        let $external = [$currentDropTargetPath, $currentDragPayload];

        shouldHighlight = isDropAllowed(currentPath);
    }
</script>

<div
    data-current-path={currentPath}
    class={cn("min-w-full", shouldHighlight ? "bg-accent/60" : "", propsClass)}
>
    <div
        tabindex={0}
        role="button"
        aria-grabbed="false"
        draggable="false"
        on:dragover={event =>
            handleDragOver(event, { type: "collection", relativePath: currentPath })}
        on:drop={handleDrop}
        on:dragend={handleDragEnd}
        on:keydown={keyboardEvent => {
            if (keyboardEvent.key === "Enter" || keyboardEvent.key === " ") {
                keyboardEvent.preventDefault();
                root.meta.is_open = !root.meta.is_open;
            }
        }}
        class={cn(
            "flex h-[22px] w-full items-center justify-between gap-2 overflow-hidden text-ellipsis whitespace-nowrap bg-background ring-inset focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        )}
        on:click={() => {
            root.meta.is_open = !root.meta.is_open;
        }}
    >
        <div class="flex h-full items-center gap-1 pl-1.5">
            {#if root.meta.is_open}
                <ChevronDownIcon size={12} class="min-h-[12px] min-w-[12px]" />
            {:else}
                <ChevronRightIcon size={12} class="min-h-[12px] min-w-[12px]" />
            {/if}
            <span class="truncate">
                {root.meta.display_name ?? root.meta.folder_name}
            </span>
        </div>
        <div class="mr-1.5 hidden items-center gap-1 group-hover/explorer:flex">
            <Tooltip group openDelay={500} closeDelay={0}>
                <TooltipTrigger asChild let:builder>
                    <Button
                        builders={[builder]}
                        variant="ghost-hover"
                        size="icon"
                        on:click={event => {
                            event.stopImmediatePropagation();
                        }}
                        class="flex size-5 max-h-5 min-h-5 min-w-5 max-w-5 flex-shrink-0 items-center justify-center"
                    >
                        <FilePlusIcon size={14} />
                    </Button>
                </TooltipTrigger>
                <TooltipContent>
                    <p>New Request</p>
                </TooltipContent>
            </Tooltip>

            <Tooltip group openDelay={500} closeDelay={0}>
                <TooltipTrigger asChild let:builder>
                    <Button
                        builders={[builder]}
                        variant="ghost-hover"
                        size="icon"
                        on:click={event => {
                            event.stopImmediatePropagation();
                        }}
                        class="flex size-5 max-h-5 min-h-5 min-w-5 max-w-5 flex-shrink-0 items-center justify-center"
                    >
                        <FolderPlusIcon size={14} />
                    </Button>
                </TooltipTrigger>
                <TooltipContent>
                    <p>New Collection</p>
                </TooltipContent>
            </Tooltip>
        </div>
    </div>
    {#if root.meta.is_open}
        <div
            class="flex h-[calc(100vh-32px-47px-36px-22px-38px)] max-h-[calc(100vh-32px-47px-36px-22px-38px)] w-full flex-1 flex-col overflow-y-auto"
        >
            <slot />
            <div
                class="min-h-8 w-full flex-grow cursor-default"
                tabindex={0}
                role="button"
                aria-grabbed="false"
                draggable="false"
                on:dragover={event =>
                    handleDragOver(event, { type: "collection", relativePath: currentPath })}
                on:drop={handleDrop}
                on:dragend={handleDragEnd}
            />
        </div>
    {/if}
</div>
