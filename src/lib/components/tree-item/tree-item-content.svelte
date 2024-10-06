<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "lucide-svelte";

    import {
        TreeItemContent,
        buildPath,
        handleDragEnd,
        handleDragOver,
        handleDragStart,
        handleDrop,
        isCurrentCollectionOrAnyOfItsChildFocussed,
        isDropAllowed,
    } from ".";
    import { type TreeItem, type DragOverDto, TREE_ITEM_TYPE } from "$lib/models";
    import {
        createNewTreeItem,
        currentDragPayload,
        currentDropTargetPath,
        focussedTreeItem,
    } from "$lib/store";
    import { cn, getMethodColorClass } from "$lib/utils/style";
    import { CollectionIcon } from "$lib/components/icons";
    import { isCollection } from "$lib/utils/tree";
    import TreeItemCreate from "./tree-item-create.svelte";
    import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";

    export let parentPath: string;
    export let currentPath: string;
    export let treeItem: TreeItem;
    export let level: number;

    let propsClass = $$props["class"];
    let shouldRenderCreateNewRequestInput = false;
    let shouldRenderCreateNewCollectionInput = false;
    let shouldHighlight = isDropAllowed(currentPath);

    const dragOverDto: DragOverDto = isCollection(treeItem)
        ? { type: "collection", relativePath: currentPath }
        : { type: "request", parentRelativePath: parentPath };

    $: {
        let $external = [$currentDropTargetPath, $currentDragPayload];

        shouldHighlight = isDropAllowed(currentPath);
    }

    $: {
        let $external = [$focussedTreeItem];

        shouldRenderCreateNewRequestInput =
            $createNewTreeItem === TREE_ITEM_TYPE.Request &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath);
    }

    $: {
        let $external = [$focussedTreeItem];

        shouldRenderCreateNewCollectionInput =
            $createNewTreeItem === TREE_ITEM_TYPE.Collection &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath);
    }
</script>

<div
    data-parent-path={parentPath}
    data-current-path={currentPath}
    class={cn("relative min-w-full", shouldHighlight ? "bg-accent/60" : "", propsClass)}
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
        class={cn(
            "flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            $focussedTreeItem.relativePath === currentPath ? "bg-accent" : "hover:bg-accent/60",
        )}
        on:click={() => {
            createNewTreeItem.set(null);

            if (isCollection(treeItem)) {
                treeItem.meta.is_open = !treeItem.meta.is_open;
                focussedTreeItem.set({
                    type: TREE_ITEM_TYPE.Collection,
                    parentRelativePath: parentPath,
                    relativePath: currentPath,
                });
            } else {
                focussedTreeItem.set({
                    type: TREE_ITEM_TYPE.Request,
                    parentRelativePath: parentPath,
                    relativePath: currentPath,
                });
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
            {#each treeItem.collections as collection (buildPath(currentPath, collection.meta.folder_name))}
                <TreeItemContent
                    parentPath={currentPath}
                    currentPath={buildPath(currentPath, collection.meta.folder_name)}
                    treeItem={collection}
                    level={level + 1}
                />
            {/each}
        {/if}
    {/if}
</div>
