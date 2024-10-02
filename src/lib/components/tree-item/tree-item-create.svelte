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
    import type { ValueOf } from "$lib/utils";

    export let name: string;
    export let type: ValueOf<typeof TREE_ITEM_TYPE>;
    export let level: number;

    let propsClass = $$props["class"];

    function init(element: HTMLElement) {
        element.focus();
    }

    $: console.log({ name });
</script>

<div class={cn("relative min-w-full", propsClass)}>
    {#if level > 1}
        <div
            style="left: {level * 8 + 3.5}px;"
            class="pointer-events-none absolute z-10 h-full w-px bg-transparent group-hover/explorer:bg-border/80"
        />
    {/if}
    <div
        style="padding-left: {level * 8}px"
        class="flex h-[22px] w-full items-center gap-2 overflow-hidden text-ellipsis whitespace-nowrap ring-inset hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
    >
        <div class="flex h-full w-full items-center gap-1 pl-1.5">
            {#if type === TREE_ITEM_TYPE.Collection}
                <div class="w-[12px] min-w-[12px]" />
                <CollectionIcon size={12} class="min-h-[12px] min-w-[12px]" />
            {:else}
                <span class={cn("pl-3 text-[9px] font-bold", getMethodColorClass("GET"))}>GET</span>
            {/if}
            <input
                use:init
                on:focusout={() => {
                    createNewTreeItem.set(null);
                    name = "";
                }}
                class="w-full whitespace-nowrap text-sm ring-inset hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                bind:value={name}
            />

            <!-- <span
                    class={cn(
                        "pl-3 text-[9px] font-bold",
                        getMethodColorClass(treeItem.config.method),
                    )}
                >
                    {treeItem.config.method}
                </span>
                <span class="truncate text-sm">
                    {treeItem.meta.display_name ?? treeItem.meta.file_name}
                </span> -->
        </div>
    </div>
</div>
