<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "lucide-svelte";

    import { TreeItemContent, TreeItemCreate } from ".";
    import { type TreeItem, type DragOverDto, TREE_ITEM_TYPE } from "$lib/models";
    import { treeActionsState } from "$lib/state.svelte";
    import { cn, getMethodColorClass } from "$lib/utils/style";
    import { CollectionIcon } from "$lib/components/icons";
    import {
        isCurrentCollectionOrAnyOfItsChildFocussed,
        isDropAllowed,
        handleDragStart,
        handleDragOver,
        handleDrop,
        handleDragEnd,
        buildPath,
        isCollection,
    } from "$lib/components/tree-item/utils.svelte";

    type Props = {
        parentPath: string;
        currentPath: string;
        treeItem: TreeItem;
        level: number;
        class?: string;
    };

    let { parentPath, currentPath, treeItem, level, class: className }: Props = $props();

    let shouldRenderCreateNewRequestInput = $derived(
        treeActionsState.createNewItem === TREE_ITEM_TYPE.Request &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath),
    );
    let shouldRenderCreateNewCollectionInput = $derived(
        treeActionsState.createNewItem === TREE_ITEM_TYPE.Collection &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath),
    );
    let shouldHighlight = $derived(isDropAllowed(currentPath));

    const dragOverDto: DragOverDto = isCollection(treeItem)
        ? { type: "collection", relativePath: currentPath }
        : { type: "request", parentRelativePath: parentPath };
</script>

<div
    data-parent-path={parentPath}
    data-current-path={currentPath}
    class={cn("relative min-w-full", shouldHighlight ? "bg-accent/60" : "", className)}
>
    {#if level > 1}
        <div
            style="left: {level * 8 + 3.5}px;"
            class="pointer-events-none absolute z-10 h-full w-px bg-transparent group-hover/explorer:bg-border/80"
        ></div>
    {/if}
    <div
        tabindex={0}
        role="button"
        aria-grabbed="false"
        draggable="true"
        ondragstart={event => {
            handleDragStart(event, { parentRelativePath: parentPath, treeItem });
        }}
        ondragover={event => handleDragOver(event, dragOverDto)}
        ondrop={handleDrop}
        ondragend={handleDragEnd}
        onkeydown={keyboardEvent => {
            if (keyboardEvent.key === "Enter" || keyboardEvent.key === " ") {
                keyboardEvent.preventDefault();
                if (isCollection(treeItem)) {
                    treeItem.meta.is_open = !treeItem.meta.is_open;
                }
            }
        }}
        style="padding-left: {level * 8}px"
        class={cn(
            "flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            treeActionsState.focussedItem.relativePath === currentPath
                ? "bg-accent"
                : "hover:bg-accent/60",
        )}
        onclick={() => {
            treeActionsState.createNewItem = null;

            if (isCollection(treeItem)) {
                treeItem.meta.is_open = !treeItem.meta.is_open;
                treeActionsState.focussedItem = {
                    type: TREE_ITEM_TYPE.Collection,
                    parentRelativePath: parentPath,
                    relativePath: currentPath,
                };
            } else {
                treeActionsState.focussedItem = {
                    type: TREE_ITEM_TYPE.Request,
                    parentRelativePath: parentPath,
                    relativePath: currentPath,
                };
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
                    {treeItem.meta.display_name ?? treeItem.meta.dir_name}
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

    {#if isCollection(treeItem)}
        {#if shouldRenderCreateNewRequestInput}
            <TreeItemCreate
                type={TREE_ITEM_TYPE.Request}
                parentRelativePath={currentPath}
                level={level + 1}
            />
        {/if}

        {#if treeItem.meta.is_open}
            {#each treeItem.requests as request (buildPath(currentPath, request.meta.file_name))}
                <TreeItemContent
                    parentPath={currentPath}
                    currentPath={buildPath(currentPath, request.meta.file_name)}
                    treeItem={request}
                    level={level + 1}
                />
            {/each}
        {/if}

        {#if shouldRenderCreateNewCollectionInput}
            <TreeItemCreate
                type={TREE_ITEM_TYPE.Collection}
                parentRelativePath={currentPath}
                level={level + 1}
            />
        {/if}
        {#if treeItem.meta.is_open}
            {#each treeItem.collections as collection (buildPath(currentPath, collection.meta.dir_name))}
                <TreeItemContent
                    parentPath={currentPath}
                    currentPath={buildPath(currentPath, collection.meta.dir_name)}
                    treeItem={collection}
                    level={level + 1}
                />
            {/each}
        {/if}
    {/if}
</div>
