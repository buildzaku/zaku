<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "lucide-svelte";

    import {
        TreeItemContent,
        handleDragEnd,
        handleDragOver,
        handleDragStart,
        handleDrop,
        isDropAllowed,
    } from ".";
    import type { TreeItem, DragOverDto } from "$lib/models";
    import { currentDragPayload, currentDropTargetPath } from "$lib/store";
    import { cn, getMethodColorClass } from "$lib/utils/style";
    import { CollectionIcon } from "$lib/components/icons";
    import { isCollection } from "$lib/utils/tree";

    export let parentPath: string;
    export let currentPath: string;
    export let treeItem: TreeItem;
    export let level: number;

    let propsClass = $$props["class"];
    let shouldHighlight = isDropAllowed(currentPath);

    const dragOverDto: DragOverDto = isCollection(treeItem)
        ? { type: "collection", relativePath: currentPath }
        : { type: "request", parentRelativePath: parentPath };

    $: {
        $currentDropTargetPath;
        $currentDragPayload;

        shouldHighlight = isDropAllowed(currentPath);
    }
</script>

<div
    data-parent-path={parentPath}
    data-current-path={currentPath}
    class={cn("relative min-w-full", shouldHighlight ? "bg-muted/50" : "", propsClass)}
>
    {#if level > 1}
        <div
            style="left: {level * 8 + 3.5}px;"
            class="pointer-events-none absolute z-10 h-full w-px bg-transparent group-hover/explorer:bg-border/80"
        />
    {/if}
    <div
        tabindex={0}
        role="button"
        aria-grabbed="false"
        draggable="true"
        on:dragstart={event => {
            handleDragStart(event, { parentRelativePath: parentPath, treeItem });
        }}
        on:dragover={event => handleDragOver(event, dragOverDto)}
        on:drop={handleDrop}
        on:dragend={handleDragEnd}
        on:keydown={keyboardEvent => {
            if (keyboardEvent.key === "Enter" || keyboardEvent.key === " ") {
                keyboardEvent.preventDefault();
                if (isCollection(treeItem)) {
                    treeItem.meta.is_open = !treeItem.meta.is_open;
                }
            }
        }}
        style="padding-left: {level * 8}px"
        class="flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        on:click={() => {
            if (isCollection(treeItem)) {
                treeItem.meta.is_open = !treeItem.meta.is_open;
            }
        }}
    >
        <div class="flex h-full items-center gap-1 pl-1.5">
            {#if isCollection(treeItem)}
                {#if treeItem.meta.is_open}
                    <ChevronDownIcon size={12} class="min-h-[12px] min-w-[12px]" />
                {:else}
                    <ChevronRightIcon size={12} class="min-h-[12px] min-w-[12px]" />
                {/if}
                <CollectionIcon size={12} />
                <span class="truncate text-sm">
                    {treeItem.meta.display_name ?? treeItem.meta.folder_name}
                </span>
            {:else}
                <span
                    class={cn(
                        "pl-3 text-[9px] font-bold",
                        getMethodColorClass(treeItem.config.method),
                    )}
                >
                    {treeItem.config.method}
                </span>
                <span class="truncate text-sm">
                    {treeItem.meta.display_name ?? treeItem.meta.file_name}
                </span>
            {/if}
        </div>
    </div>

    {#if isCollection(treeItem) && treeItem.meta.is_open}
        {#each treeItem.requests as request (`${currentPath}/${request.meta.file_name}`)}
            <TreeItemContent
                parentPath={currentPath}
                currentPath={`${currentPath}/${request.meta.file_name}`}
                treeItem={request}
                level={level + 1}
            />
        {/each}
        {#each treeItem.collections as collection (`${currentPath}/${collection.meta.folder_name}`)}
            <TreeItemContent
                parentPath={currentPath}
                currentPath={`${currentPath}/${collection.meta.folder_name}`}
                treeItem={collection}
                level={level + 1}
            />
        {/each}
    {/if}
</div>
