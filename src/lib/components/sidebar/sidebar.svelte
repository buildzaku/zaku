<script lang="ts">
    import { zakuState, treeActionsState } from "$lib/state.svelte";
    import { Button } from "$lib/components/primitives/button";
    import { CookieIcon, SettingsIcon, ChevronsLeftIcon, CompassIcon } from "lucide-svelte";
    import type { PaneAPI } from "paneforge";

    import { SpaceSwitcher } from "$lib/components/space";
    import { cn } from "$lib/utils/style";
    import { TreeItemContent, TreeItemCreate, TreeItemRoot } from "$lib/components/tree-item";
    import {
        Tooltip,
        TooltipTrigger,
        TooltipContent,
        TooltipProvider,
    } from "$lib/components/primitives/tooltip";
    import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
    import { TREE_ITEM_TYPE } from "$lib/models";
    import { isCurrentCollectionOrAnyOfItsChildFocussed } from "$lib/components/tree-item/utils.svelte";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
    };

    let { pane, isCollapsed = $bindable() }: Props = $props();

    let shouldRenderCreateNewRequestInput = $derived(
        treeActionsState.createNewItem === TREE_ITEM_TYPE.Request &&
            isCurrentCollectionOrAnyOfItsChildFocussed(RELATIVE_SPACE_ROOT),
    );
    let shouldRenderCreateNewCollectionInput = $derived(
        treeActionsState.createNewItem === TREE_ITEM_TYPE.Collection &&
            isCurrentCollectionOrAnyOfItsChildFocussed(RELATIVE_SPACE_ROOT),
    );
    let treeItemInputName = $state("");
</script>

{#if zakuState.activeSpace}
    <div class="flex size-full flex-col justify-between">
        <div class="flex w-full items-center justify-center border-b p-1.5 pt-0">
            <div class={cn("flex w-full items-center justify-between gap-1.5")}>
                <div class="flex-grow overflow-hidden text-ellipsis whitespace-nowrap">
                    <SpaceSwitcher isSidebarCollapsed={isCollapsed} />
                </div>
                {#if !isCollapsed}
                    <Button
                        variant="ghost"
                        size="icon"
                        onclick={() => {
                            if (isCollapsed) {
                                pane.expand();
                                pane.resize(24);
                            } else {
                                pane.collapse();
                            }
                        }}
                        class="flex-shrink-0"
                    >
                        <ChevronsLeftIcon size={16} class="min-h-[14px] min-w-[14px]" />
                    </Button>
                {/if}
            </div>
        </div>
        <div class="group/explorer flex w-full grow items-start justify-center overflow-y-auto">
            {#if isCollapsed}
                <TooltipProvider>
                    <Tooltip delayDuration={500}>
                        <TooltipTrigger>
                            <Button
                                variant="ghost"
                                size="icon"
                                onclick={() => {
                                    pane.expand();
                                    pane.resize(24);
                                }}
                                class="my-1.5 flex-shrink-0"
                            >
                                <CompassIcon size={14} class="min-h-[14px] min-w-[14px]" />
                            </Button>
                        </TooltipTrigger>
                        <TooltipContent side="right">Explorer</TooltipContent>
                    </Tooltip>
                </TooltipProvider>
            {:else}
                <div class="size-full">
                    <p class="flex h-[36px] items-center px-[22px] text-muted-foreground">
                        Explorer
                    </p>
                    <TreeItemRoot
                        currentPath={RELATIVE_SPACE_ROOT}
                        root={zakuState.activeSpace.root}
                    >
                        {#if shouldRenderCreateNewRequestInput}
                            <TreeItemCreate
                                type={TREE_ITEM_TYPE.Request}
                                parentRelativePath={RELATIVE_SPACE_ROOT}
                                level={1}
                            />
                        {/if}
                        {#each zakuState.activeSpace.root.requests as request (request.meta.file_name)}
                            <TreeItemContent
                                parentPath={RELATIVE_SPACE_ROOT}
                                currentPath={request.meta.file_name}
                                treeItem={request}
                                level={1}
                            />
                        {/each}

                        {#if shouldRenderCreateNewCollectionInput}
                            <TreeItemCreate
                                type={TREE_ITEM_TYPE.Collection}
                                parentRelativePath={RELATIVE_SPACE_ROOT}
                                level={1}
                            />
                        {/if}
                        {#each zakuState.activeSpace.root.collections as collection (collection.meta.dir_name)}
                            <TreeItemContent
                                parentPath={RELATIVE_SPACE_ROOT}
                                currentPath={collection.meta.dir_name}
                                treeItem={collection}
                                level={1}
                            />
                        {/each}
                    </TreeItemRoot>
                </div>
            {/if}
        </div>
        <div
            class={cn(
                "flex items-center justify-between gap-1.5 border-t p-1.5",
                isCollapsed && "flex-col-reverse",
            )}
        >
            <Button size="icon" variant="ghost">
                <SettingsIcon strokeWidth={1.25} size={16} />
                <span class="sr-only">Settings</span>
            </Button>
            <Button size="icon" variant="ghost">
                <CookieIcon size={14} />
            </Button>
        </div>
    </div>
{/if}
