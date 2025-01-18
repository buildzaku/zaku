<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "lucide-svelte";

    import { handleDragOver, handleDrop, handleDragEnd, isDropAllowed } from ".";
    import type { Collection } from "$lib/bindings";
    import {
        createNewTreeItem,
        currentDragPayload,
        currentDropTargetPath,
        focussedTreeItem,
    } from "$lib/store";
    import { cn } from "$lib/utils/style";
    import { Button } from "$lib/components/primitives/button";
    import { FilePlusIcon, FolderPlusIcon } from "$lib/components/icons";
    import { Tooltip, TooltipTrigger, TooltipContent } from "$lib/components/primitives/tooltip";
    import { TREE_ITEM_TYPE } from "$lib/models";
    import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";

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
                focussedTreeItem.set({
                    type: TREE_ITEM_TYPE.Collection,
                    relativePath: RELATIVE_SPACE_ROOT,
                    parentRelativePath: RELATIVE_SPACE_ROOT,
                });
            }
        }}
        class={cn(
            "flex h-[22px] w-full items-center justify-between gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus:outline-none focus:ring-1 focus:ring-ring",
        )}
        on:click={() => {
            root.meta.is_open = !root.meta.is_open;
            focussedTreeItem.set({
                type: TREE_ITEM_TYPE.Collection,
                relativePath: RELATIVE_SPACE_ROOT,
                parentRelativePath: RELATIVE_SPACE_ROOT,
            });
        }}
    >
        <div class="flex h-full items-center gap-1 pl-1.5">
            {#if root.meta.is_open}
                <ChevronDownIcon size={12} class="min-h-[12px] min-w-[12px]" />
            {:else}
                <ChevronRightIcon size={12} class="min-h-[12px] min-w-[12px]" />
            {/if}
            <span class="truncate">
                {root.meta.display_name ?? root.meta.dir_name}
            </span>
        </div>

        {#if root.meta.is_open}
            <div
                role="button"
                tabindex={-1}
                on:click={event => {
                    event.stopImmediatePropagation();
                }}
                on:keydown={keyboardEvent => {
                    keyboardEvent.stopImmediatePropagation();
                }}
                class="hidden h-full items-center gap-1 px-1.5 group-hover/explorer:flex"
            >
                <Tooltip group openDelay={500} closeDelay={0}>
                    <TooltipTrigger asChild let:builder>
                        <Button
                            builders={[builder]}
                            data-create-tree-item-button
                            variant="ghost-hover"
                            size="icon"
                            on:click={event => {
                                event.stopImmediatePropagation();
                                createNewTreeItem.set(TREE_ITEM_TYPE.Request);
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
                            data-create-tree-item-button
                            variant="ghost-hover"
                            size="icon"
                            on:click={event => {
                                event.stopImmediatePropagation();
                                createNewTreeItem.set(TREE_ITEM_TYPE.Collection);
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
        {/if}
    </div>

    {#if root.meta.is_open}
        <div
            class="flex h-[calc(100dvh-36px-35px-36px-22px-37px)] max-h-[calc(100dvh-36px-35px-36px-22px-37px)] w-full flex-1 flex-col overflow-y-auto"
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
            ></div>
        </div>
    {/if}
</div>
