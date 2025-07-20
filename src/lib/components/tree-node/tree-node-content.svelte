<script lang="ts">
    import { ChevronDownIcon, ChevronRightIcon } from "@lucide/svelte";
    import { toast } from "svelte-sonner";

    import { TreeNodeContent, TreeNodeCreate } from ".";
    import type { TreeNode, DragOverDto } from "$lib/models";
    import { explorerActionsState, explorerState } from "$lib/state.svelte";
    import { cn, requestColors } from "$lib/utils/style";
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
    } from "$lib/components/tree-node/utils.svelte";

    type Props = {
        parentRelpath: string;
        parentNames: string[];
        currentPath: string;
        node: TreeNode;
        level: number;
        class?: string;
    };

    let {
        parentRelpath,
        parentNames,
        currentPath,
        node,
        level,
        class: className,
    }: Props = $props();

    let shouldRenderCreateNewRequestInput = $derived(
        explorerActionsState.createNewNode === "request" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath),
    );
    let shouldRenderCreateNewCollectionInput = $derived(
        explorerActionsState.createNewNode === "collection" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(currentPath),
    );
    let shouldHighlight = $derived(isDropAllowed(currentPath));

    const dragOverDto: DragOverDto = isCol(node)
        ? { type: "collection", relativePath: currentPath }
        : { type: "request", parentRelativePath: parentRelpath };

    type TreeNodeFocusParams = { node: TreeNode; parentRelpath: string; relpath: string };
    function handleTreeItemFocus({ node, parentRelpath, relpath }: TreeNodeFocusParams) {
        if (isCol(node)) {
            node.meta.is_expanded = !node.meta.is_expanded;

            explorerState.focussedNode = {
                type: "collection",
                parentRelativePath: parentRelpath,
                relativePath: relpath,
            };
        } else if (isReq(node)) {
            explorerState.focussedNode = {
                type: "request",
                parentRelativePath: parentRelpath,
                relativePath: relpath,
            };

            explorerState.openRequest = {
                parentRelpath: parentRelpath,
                parentNames,
                self: node,
            };

            if (!explorerState.backgroundRequests.includes(node)) {
                explorerState.backgroundRequests.push(node);
            }
        } else {
            toast.error("Something went wrong while trying to focus on item");
        }
    }
</script>

<div
    data-parent-path={parentRelpath}
    data-current-path={currentPath}
    class={cn("relative min-w-full", shouldHighlight ? "bg-accent/75" : "", className)}
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
            handleDragStart(event, { parentRelativePath: parentRelpath, node });
        }}
        ondragover={event => handleDragOver(event, dragOverDto)}
        ondrop={handleDrop}
        ondragend={handleDragEnd}
        onkeydown={keyboardEvent => {
            if (keyboardEvent.key === "Enter" || keyboardEvent.key === " ") {
                keyboardEvent.preventDefault();

                handleTreeItemFocus({ node, parentRelpath: parentRelpath, relpath: currentPath });
            }
        }}
        style="padding-left: {level * 8}px"
        class={cn(
            "focus-visible:ring-ring flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus-visible:ring-1 focus-visible:outline-none",
            explorerState.focussedNode.relativePath === currentPath
                ? "bg-accent"
                : "hover:bg-accent/75",
        )}
        onclick={() => {
            explorerActionsState.createNewNode = null;

            handleTreeItemFocus({ node, parentRelpath: parentRelpath, relpath: currentPath });
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
                <span class="text-small truncate">
                    {node.meta.name ?? node.meta.fsname}
                </span>
            {:else}
                <div class="flex w-full items-center justify-between">
                    <div>
                        <span
                            class={cn(
                                "pl-3 text-[9px] font-bold",
                                requestColors({ method: node.config.method }),
                            )}
                        >
                            {node.config.method}
                        </span>
                        <span class="text-small truncate">
                            {node.meta.name ?? node.meta.fsname}
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
            <TreeNodeCreate type="request" parentRelativePath={currentPath} level={level + 1} />
        {/if}

        {#if node.meta.is_expanded}
            {#each node.requests as request (buildPath(currentPath, request.meta.fsname))}
                <TreeNodeContent
                    parentRelpath={currentPath}
                    parentNames={[...parentNames, node.meta.name ?? node.meta.fsname]}
                    currentPath={buildPath(currentPath, request.meta.fsname)}
                    node={request}
                    level={level + 1}
                />
            {/each}
        {/if}

        {#if shouldRenderCreateNewCollectionInput}
            <TreeNodeCreate type="collection" parentRelativePath={currentPath} level={level + 1} />
        {/if}
        {#if node.meta.is_expanded}
            {#each node.collections as collection (buildPath(currentPath, collection.meta.fsname))}
                <TreeNodeContent
                    parentRelpath={currentPath}
                    parentNames={[...parentNames, node.meta.name ?? node.meta.fsname]}
                    currentPath={buildPath(currentPath, collection.meta.fsname)}
                    node={collection}
                    level={level + 1}
                />
            {/each}
        {/if}
    {/if}
</div>
