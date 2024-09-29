<script lang="ts">
    import { zakuState } from "$lib/store";
    import { Button } from "$lib/components/primitives/button";
    import { CookieIcon, SettingsIcon, ChevronsLeftIcon, CompassIcon } from "lucide-svelte";
    import type { PaneAPI } from "paneforge";

    import { SpaceSwitcher } from "$lib/components/space";
    import { cn } from "$lib/utils/style";
    import { TreeItemContent, TreeItemRoot } from "$lib/components/tree-item";
    import { Tooltip, TooltipTrigger, TooltipContent } from "$lib/components/primitives/tooltip";
    import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";

    export let pane: PaneAPI;
    export let isCollapsed: boolean;
</script>

{#if $zakuState.active_space}
    <div class="flex size-full flex-col justify-between">
        <div class="mt-1.5 flex w-full items-center justify-center border-b p-1.5">
            <div class={cn("flex w-full items-center justify-between gap-1.5")}>
                <div class="flex-grow overflow-hidden text-ellipsis whitespace-nowrap">
                    <SpaceSwitcher isSidebarCollapsed={isCollapsed} />
                </div>
                {#if !isCollapsed}
                    <Button
                        variant="ghost"
                        size="icon"
                        on:click={() => {
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
        <div class="group/explorer flex w-full grow justify-center overflow-y-auto">
            {#if isCollapsed}
                <Tooltip group openDelay={500} closeDelay={0}>
                    <TooltipTrigger asChild let:builder>
                        <Button
                            builders={[builder]}
                            variant="ghost"
                            size="icon"
                            on:click={() => {
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
            {:else}
                <div class="size-full">
                    <p
                        class="flex h-[36px] items-center bg-background px-[22px] text-muted-foreground"
                    >
                        Explorer
                    </p>
                    <TreeItemRoot
                        currentPath={RELATIVE_SPACE_ROOT}
                        root={$zakuState.active_space.root}
                    >
                        {#each $zakuState.active_space.root.requests as request (`/${request.meta.file_name}`)}
                            <TreeItemContent
                                parentPath={RELATIVE_SPACE_ROOT}
                                currentPath={`/${request.meta.file_name}`}
                                treeItem={request}
                                level={1}
                            />
                        {/each}
                        {#each $zakuState.active_space.root.collections as collection (`/${collection.meta.folder_name}`)}
                            <TreeItemContent
                                parentPath={RELATIVE_SPACE_ROOT}
                                currentPath={`/${collection.meta.folder_name}`}
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
