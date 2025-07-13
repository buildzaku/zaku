<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "@lucide/svelte";
    import { toast } from "svelte-sonner";

    import { TreeItemContent, TreeItemCreate } from ".";
    import { type TreeItem, type DragOverDto, TreeItemType } from "$lib/models";
    import { treeActionsState, treeItemsState } from "$lib/state.svelte";
    import { cn, getMethodColorClass } from "$lib/utils/style";
    import { CollectionIcon, DotIcon } from "$lib/components/icons";
    import {
        isCurrentCollectionOrAnyOfItsChildFocussed,
        isDropAllowed,
        handleDragStart,
        handleDragOver,
        handleDrop,
        handleDragEnd,
        buildPath,
        isCol,
        isReq,
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
        treeActionsState.createNewItem === "request" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath),
    );
    let shouldRenderCreateNewCollectionInput = $derived(
        treeActionsState.createNewItem === "collection" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath),
    );
    let shouldHighlight = $derived(isDropAllowed(currentPath));

    const dragOverDto: DragOverDto = isCol(treeItem)
        ? { type: TreeItemType.Collection, relativePath: currentPath }
        : { type: TreeItemType.Request, parentRelativePath: parentPath };

    type TreeItemFocusParams = { treeItem: TreeItem; parentRelpath: string; relpath: string };
    function handleTreeItemFocus({ treeItem, parentRelpath, relpath }: TreeItemFocusParams) {
        if (isCol(treeItem)) {
            treeItem.meta.is_expanded = !treeItem.meta.is_expanded;

            treeItemsState.focussedItem = {
                type: TreeItemType.Collection,
                parentRelativePath: parentRelpath,
                relativePath: relpath,
            };
        } else if (isReq(treeItem)) {
            treeItemsState.focussedItem = {
                type: TreeItemType.Request,
                parentRelativePath: parentRelpath,
                relativePath: relpath,
            };

            treeItemsState.activeRequest = {
                parentRelativePath: parentRelpath,
                self: treeItem,
            };

            if (!treeItemsState.openRequests.includes(treeItem)) {
                treeItemsState.openRequests.push(treeItem);
            }
        } else {
            toast.error("Something went wrong while trying to focus on item");
        }
    }
</script>

<div
    data-parent-path={parentPath}
    data-current-path={currentPath}
    class={cn("relative min-w-full", shouldHighlight ? "bg-accent/60" : "", className)}
>
    {#if level > 1}
        <div
            style="left: {level * 8 + 3.5}px;"
            class="group-hover/explorer:bg-border/80 pointer-events-none absolute z-10 h-full w-px bg-transparent"
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

                handleTreeItemFocus({ treeItem, parentRelpath: parentPath, relpath: currentPath });
            }
        }}
        style="padding-left: {level * 8}px"
        class={cn(
            "focus-visible:ring-ring flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus-visible:ring-1 focus-visible:outline-none",
            treeItemsState.focussedItem.relativePath === currentPath
                ? "bg-accent"
                : "hover:bg-accent/60",
        )}
        onclick={() => {
            treeActionsState.createNewItem = null;

            handleTreeItemFocus({ treeItem, parentRelpath: parentPath, relpath: currentPath });
        }}
    >
        <div class="flex size-full items-center gap-1 pl-1.5">
            {#if isCol(treeItem)}
                {#if treeItem.meta.is_expanded}
                    <ChevronDownIcon size={12} class="min-h-[12px] min-w-[12px]" />
                {:else}
                    <ChevronRightIcon size={12} class="min-h-[12px] min-w-[12px]" />
                {/if}
                <CollectionIcon size={12} />
                <span class="truncate text-sm">
                    {treeItem.meta.display_name ?? treeItem.meta.dir_name}
                </span>
            {:else}
                <div class="flex w-full items-center justify-between">
                    <div>
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
                    </div>
                    {#if treeItem.meta.has_unsaved_changes}
                        <DotIcon size={6} class="fill-primary/80 mr-2.5" />
                    {/if}
                </div>
            {/if}
        </div>
    </div>

    {#if isCol(treeItem)}
        {#if shouldRenderCreateNewRequestInput}
            <TreeItemCreate
                type={TreeItemType.Request}
                parentRelativePath={currentPath}
                level={level + 1}
            />
        {/if}

        {#if treeItem.meta.is_expanded}
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
                type={TreeItemType.Collection}
                parentRelativePath={currentPath}
                level={level + 1}
            />
        {/if}
        {#if treeItem.meta.is_expanded}
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
