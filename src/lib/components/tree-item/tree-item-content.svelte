<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "@lucide/svelte";
    import { toast } from "svelte-sonner";

    import { TreeNodeContent, TreeNodeCreate } from ".";
    import type { TreeNode, DragOverDto } from "$lib/models";
    import { treeActionsState, treeNodesState } from "$lib/state.svelte";
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
        node: TreeNode;
        level: number;
        class?: string;
    };

    let { parentPath, currentPath, node, level, class: className }: Props = $props();

    let shouldRenderCreateNewRequestInput = $derived(
        treeActionsState.createNewNode === "request" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath),
    );
    let shouldRenderCreateNewCollectionInput = $derived(
        treeActionsState.createNewNode === "collection" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath),
    );
    let shouldHighlight = $derived(isDropAllowed(currentPath));

    const dragOverDto: DragOverDto = isCol(node)
        ? { type: "collection", relativePath: currentPath }
        : { type: "request", parentRelativePath: parentPath };

    type TreeNodeFocusParams = { node: TreeNode; parentRelpath: string; relpath: string };
    function handleTreeItemFocus({ node, parentRelpath, relpath }: TreeNodeFocusParams) {
        if (isCol(node)) {
            node.meta.is_expanded = !node.meta.is_expanded;

            treeNodesState.focussedNode = {
                type: "collection",
                parentRelativePath: parentRelpath,
                relativePath: relpath,
            };
        } else if (isReq(node)) {
            treeNodesState.focussedNode = {
                type: "request",
                parentRelativePath: parentRelpath,
                relativePath: relpath,
            };

            treeNodesState.activeRequest = {
                parentRelativePath: parentRelpath,
                self: node,
            };

            if (!treeNodesState.openRequests.includes(node)) {
                treeNodesState.openRequests.push(node);
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
            handleDragStart(event, { parentRelativePath: parentPath, node });
        }}
        ondragover={event => handleDragOver(event, dragOverDto)}
        ondrop={handleDrop}
        ondragend={handleDragEnd}
        onkeydown={keyboardEvent => {
            if (keyboardEvent.key === "Enter" || keyboardEvent.key === " ") {
                keyboardEvent.preventDefault();

                handleTreeItemFocus({ node, parentRelpath: parentPath, relpath: currentPath });
            }
        }}
        style="padding-left: {level * 8}px"
        class={cn(
            "focus-visible:ring-ring flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus-visible:ring-1 focus-visible:outline-none",
            treeNodesState.focussedNode.relativePath === currentPath
                ? "bg-accent"
                : "hover:bg-accent/60",
        )}
        onclick={() => {
            treeActionsState.createNewNode = null;

            handleTreeItemFocus({ node, parentRelpath: parentPath, relpath: currentPath });
        }}
    >
        <div class="flex size-full items-center gap-1 pl-1.5">
            {#if isCol(node)}
                {#if node.meta.is_expanded}
                    <ChevronDownIcon size={12} class="min-h-[12px] min-w-[12px]" />
                {:else}
                    <ChevronRightIcon size={12} class="min-h-[12px] min-w-[12px]" />
                {/if}
                <CollectionIcon size={12} />
                <span class="truncate text-sm">
                    {node.meta.name ?? node.meta.dir_name}
                </span>
            {:else}
                <div class="flex w-full items-center justify-between">
                    <div>
                        <span
                            class={cn(
                                "pl-3 text-[9px] font-bold",
                                getMethodColorClass(node.config.method),
                            )}
                        >
                            {node.config.method}
                        </span>
                        <span class="truncate text-sm">
                            {node.meta.name ?? node.meta.file_name}
                        </span>
                    </div>
                    {#if node.meta.has_unsaved_changes}
                        <DotIcon size={6} class="fill-primary/80 mr-2.5" />
                    {/if}
                </div>
            {/if}
        </div>
    </div>

    {#if isCol(node)}
        {#if shouldRenderCreateNewRequestInput}
            <TreeNodeCreate type={"request"} parentRelativePath={currentPath} level={level + 1} />
        {/if}

        {#if node.meta.is_expanded}
            {#each node.requests as request (buildPath(currentPath, request.meta.file_name))}
                <TreeNodeContent
                    parentPath={currentPath}
                    currentPath={buildPath(currentPath, request.meta.file_name)}
                    node={request}
                    level={level + 1}
                />
            {/each}
        {/if}

        {#if shouldRenderCreateNewCollectionInput}
            <TreeNodeCreate
                type={"collection"}
                parentRelativePath={currentPath}
                level={level + 1}
            />
        {/if}
        {#if node.meta.is_expanded}
            {#each node.collections as collection (buildPath(currentPath, collection.meta.dir_name))}
                <TreeNodeContent
                    parentPath={currentPath}
                    currentPath={buildPath(currentPath, collection.meta.dir_name)}
                    node={collection}
                    level={level + 1}
                />
            {/each}
        {/if}
    {/if}
</div>
