<script lang="ts">
  import { ChevronDownIcon, ChevronRightIcon } from "@lucide/svelte";
  import { toast } from "svelte-sonner";

  import { TreeNodeContent, TreeNodeCreate } from ".";
  import type { TreeNode } from "$lib/models";
  import { explorerActionsState, explorerState } from "$lib/state.svelte";
  import { cn, requestColors } from "$lib/utils/style";
  import { CollectionIcon, DotIcon } from "$lib/components/icons";
  import {
    isDropAllowed,
    handleDragStart,
    handleDragOver,
    handleDrop,
    handleDragEnd,
    isCol,
    isReq,
  } from "$lib/components/tree-node/utils.svelte";
  import { Path } from "$lib/utils/path";

  type Props = { trail: string[]; node: TreeNode; level: number; class?: string };
  let { trail, node, level, class: className }: Props = $props();

  const relpath = Path.from(node.meta.relpath);

  function handleTreeItemFocus(node: TreeNode) {
    if (isCol(node)) {
      node.meta.is_expanded = !node.meta.is_expanded;
      explorerState.setFocussedNode({
        type: "collection",
        relpath: relpath,
      });
    } else if (isReq(node)) {
      explorerState.setFocussedNode({
        type: "request",
        relpath: relpath,
      });
      explorerState.setOpenRequest({
        trail,
        self: node,
      });
    } else {
      toast.error("Something went wrong while trying to focus on the node");
    }
  }
</script>

<div
  data-parent-path={relpath.parent()?.toString() ?? ""}
  data-current-path={node.meta.relpath}
  class={cn(
    "relative min-w-full",
    isDropAllowed(node.meta.relpath) ? "bg-accent/75" : "",
    className,
  )}
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
      handleDragStart(event, node);
    }}
    ondragover={event => {
      handleDragOver(event, {
        type: isCol(node) ? "collection" : "request",
        relpath: relpath,
      });
    }}
    ondrop={handleDrop}
    ondragend={handleDragEnd}
    onkeydown={keyboardEvent => {
      if (keyboardEvent.key === "Enter" || keyboardEvent.key === " ") {
        keyboardEvent.preventDefault();

        handleTreeItemFocus(node);
      }
    }}
    style="padding-left: {level * 8}px"
    class={cn(
      "focus-visible:ring-ring flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset focus-visible:ring-1 focus-visible:outline-none",
      explorerState.focussedNode.relpath.toString() === node.meta.relpath
        ? "bg-accent"
        : "hover:bg-accent/75",
    )}
    onclick={() => {
      explorerActionsState.createNewNode = null;

      handleTreeItemFocus(node);
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
              class={cn("pl-3 text-[9px] font-bold", requestColors({ method: node.config.method }))}
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
    {#if explorerActionsState.createNewNode === "request" && explorerState.isCreateNewNodeParent(relpath)}
      <TreeNodeCreate type="request" locationRelpath={relpath} level={level + 1} />
    {/if}

    {#if node.meta.is_expanded}
      {#each node.requests as request (relpath.join(request.meta.fsname).toString())}
        <TreeNodeContent
          trail={[...trail, node.meta.name ?? node.meta.fsname]}
          node={request}
          level={level + 1}
        />
      {/each}
    {/if}

    {#if explorerActionsState.createNewNode === "collection" && explorerState.isCreateNewNodeParent(relpath)}
      <TreeNodeCreate type="collection" locationRelpath={relpath} level={level + 1} />
    {/if}
    {#if node.meta.is_expanded}
      {#each node.collections as collection (relpath.join(collection.meta.fsname).toString())}
        <TreeNodeContent
          trail={[...trail, node.meta.name ?? node.meta.fsname]}
          node={collection}
          level={level + 1}
        />
      {/each}
    {/if}
  {/if}
</div>
