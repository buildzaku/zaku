<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "lucide-svelte";

    import type { Collection } from "$lib/bindings";
    import { treeActionsState } from "$lib/state.svelte";
    import { cn } from "$lib/utils/style";
    import { Button } from "$lib/components/primitives/button";
    import { FilePlusIcon, FolderPlusIcon } from "$lib/components/icons";
    import {
        Tooltip,
        TooltipTrigger,
        TooltipContent,
        TooltipProvider,
    } from "$lib/components/primitives/tooltip";
    import { TREE_ITEM_TYPE } from "$lib/models";
    import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
    import {
        handleDragEnd,
        handleDragOver,
        handleDrop,
        isDropAllowed,
    } from "$lib/components/tree-item/utils.svelte";

    type Props = { currentPath: string; root: Collection; class?: string };

    let { currentPath, root, class: className }: Props = $props();

    let shouldHighlight = $derived(isDropAllowed(currentPath));
</script>

<div
    data-current-path={currentPath}
    class={cn("min-w-full", shouldHighlight ? "bg-accent/60" : "", className)}
>
    <div
        tabindex={0}
        role="button"
        aria-grabbed="false"
        draggable="false"
        ondragover={event =>
            handleDragOver(event, { type: "collection", relativePath: currentPath })}
        ondrop={handleDrop}
        ondragend={handleDragEnd}
        onkeydown={keyboardEvent => {
            if (keyboardEvent.key === "Enter" || keyboardEvent.key === " ") {
                keyboardEvent.preventDefault();
                root.meta.is_open = !root.meta.is_open;
                treeActionsState.focussedItem = {
                    type: TREE_ITEM_TYPE.Collection,
                    relativePath: RELATIVE_SPACE_ROOT,
                    parentRelativePath: RELATIVE_SPACE_ROOT,
                };
            }
        }}
        class={cn(
            "flex h-[22px] w-full items-center justify-between gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus:outline-none focus:ring-1 focus:ring-ring",
        )}
        onclick={() => {
            root.meta.is_open = !root.meta.is_open;
            treeActionsState.focussedItem = {
                type: TREE_ITEM_TYPE.Collection,
                relativePath: RELATIVE_SPACE_ROOT,
                parentRelativePath: RELATIVE_SPACE_ROOT,
            };
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
                onclick={event => {
                    event.stopImmediatePropagation();
                }}
                onkeydown={keyboardEvent => {
                    keyboardEvent.stopImmediatePropagation();
                }}
                class="hidden h-full items-center gap-1 px-1.5 group-hover/explorer:flex"
            >
                <TooltipProvider>
                    <Tooltip delayDuration={500} disableHoverableContent>
                        <TooltipTrigger>
                            <Button
                                data-create-tree-item-button
                                variant="ghost"
                                size="icon"
                                onclick={event => {
                                    event.stopImmediatePropagation();
                                    treeActionsState.createNewItem = TREE_ITEM_TYPE.Request;
                                }}
                                class="flex items-center justify-center"
                            >
                                <FilePlusIcon
                                    size={13}
                                    class="size-[13px] max-h-[13px] max-w-[13px]"
                                />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>
                            <p>New Request</p>
                        </TooltipContent>
                    </Tooltip>
                </TooltipProvider>
                <TooltipProvider>
                    <Tooltip delayDuration={500} disableHoverableContent>
                        <TooltipTrigger>
                            <Button
                                data-create-tree-item-button
                                variant="ghost"
                                size="icon"
                                onclick={event => {
                                    event.stopImmediatePropagation();
                                    treeActionsState.createNewItem = TREE_ITEM_TYPE.Collection;
                                }}
                                class="flex items-center justify-center"
                            >
                                <FolderPlusIcon
                                    size={13}
                                    class="size-[13px] max-h-[13px] max-w-[13px]"
                                />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent>
                            <p>New Collection</p>
                        </TooltipContent>
                    </Tooltip>
                </TooltipProvider>
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
                ondragover={event =>
                    handleDragOver(event, { type: "collection", relativePath: currentPath })}
                ondrop={handleDrop}
                ondragend={handleDragEnd}
            ></div>
        </div>
    {/if}
</div>
