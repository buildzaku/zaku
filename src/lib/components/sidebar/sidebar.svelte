<script lang="ts">
    import { zakuState, treeActionsState } from "$lib/state.svelte";
    import { Button } from "$lib/components/primitives/button";
    import { CookieIcon, SettingsIcon, ChevronsLeftIcon, CompassIcon } from "@lucide/svelte";
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
    import { TreeItemType } from "$lib/models";
    import { isCurrentCollectionOrAnyOfItsChildFocussed } from "$lib/components/tree-item/utils.svelte";
    import {
        Dialog,
        DialogContent,
        DialogDescription,
        DialogHeader,
        DialogTitle,
        DialogTrigger,
    } from "$lib/components/primitives/dialog";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
    };

    let { pane, isCollapsed = $bindable() }: Props = $props();

    let shouldRenderCreateNewRequestInput = $derived(
        treeActionsState.createNewItem === "request" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(RELATIVE_SPACE_ROOT),
    );
    let shouldRenderCreateNewCollectionInput = $derived(
        treeActionsState.createNewItem === "collection" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(RELATIVE_SPACE_ROOT),
    );
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
                    <Tooltip delayDuration={500} disableHoverableContent>
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
                    <p class="text-muted-foreground flex h-[36px] items-center px-[22px]">
                        Explorer
                    </p>
                    <TreeItemRoot
                        currentPath={RELATIVE_SPACE_ROOT}
                        root={zakuState.activeSpace.root}
                    >
                        {#if shouldRenderCreateNewRequestInput}
                            <TreeItemCreate
                                type={TreeItemType.Request}
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
                                type={TreeItemType.Collection}
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
            <Dialog>
                <DialogTrigger>
                    <Button size="icon" variant="ghost">
                        <CookieIcon size={14} />
                    </Button>
                </DialogTrigger>
                <DialogContent class="h-[80%] max-h-[80%] w-[80%] max-w-[80%]">
                    <DialogHeader>
                        <DialogTitle>Cookies</DialogTitle>
                        <DialogDescription>Manage your space cookies</DialogDescription>
                    </DialogHeader>
                    <div class="h-full max-h-[calc(100%-1.5rem)]">
                        <div class="bg-card flex h-full flex-col overflow-hidden rounded border">
                            {#each Object.entries(zakuState.activeSpace.cookies) as [domain, cookies], idx (idx)}
                                <div class="p-2 font-semibold">{domain}</div>
                                {#if cookies}
                                    <div class="bg-accent/25 flex border-b font-semibold">
                                        <div class="w-[15%] border-r p-2">Name</div>
                                        <div class="w-[20%] border-r p-2">Value</div>
                                        <div class="w-[15%] border-r p-2">Domain</div>
                                        <div class="w-[10%] border-r p-2">Path</div>
                                        <div class="w-[15%] border-r p-2">Expires</div>
                                        <div class="w-[5%] border-r p-2">Size</div>
                                        <div class="w-[5%] border-r p-2">HTTP</div>
                                        <div class="w-[5%] border-r p-2">Secure</div>
                                        <div class="w-[10%] p-2">SameSite</div>
                                    </div>
                                    <div class="overflow-y-auto">
                                        {#each cookies as ck, idx (idx)}
                                            <div class="flex border-b last:border-b-0">
                                                <div class="w-[15%] p-2 break-all">{ck.name}</div>
                                                <div class="w-[20%] p-2 break-all">{ck.value}</div>
                                                <div class="w-[15%] p-2 break-all">{ck.domain}</div>
                                                <div class="w-[10%] p-2 break-all">{ck.path}</div>
                                                <div class="w-[15%] p-2 break-all select-text">
                                                    {ck.expires}
                                                </div>
                                                <div class="w-[5%] p-2 break-all">
                                                    {ck.name.length + ck.value.length}
                                                </div>
                                                <div class="w-[5%] p-2 break-all">
                                                    {ck.http_only ? "Yes" : "No"}
                                                </div>
                                                <div class="w-[5%] p-2 break-all">
                                                    {ck.secure ? "Yes" : "No"}
                                                </div>
                                                <div class="w-[10%] p-2 break-all">
                                                    {ck.same_site ?? "None"}
                                                </div>
                                            </div>
                                        {/each}
                                    </div>
                                {/if}
                            {/each}
                        </div>
                    </div>
                </DialogContent>
            </Dialog>
        </div>
    </div>
{/if}
