<script lang="ts">
    import { zakuState, treeActionsState } from "$lib/state.svelte";
    import { Button } from "$lib/components/primitives/button";
    import { CookieIcon, SettingsIcon, ChevronsLeftIcon, CompassIcon, XIcon } from "@lucide/svelte";
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
        DialogTrigger,
        DialogHeader,
        DialogTitle,
        DialogDescription,
        DialogContent,
        DialogFooter,
    } from "$lib/components/primitives/dialog";
    import {
        Accordion,
        AccordionContent,
        AccordionItem,
        AccordionTrigger,
    } from "$lib/components/primitives/accordion";
    import { Badge } from "$lib/components/primitives/badge";
    import { commands } from "$lib/bindings";
    import type { RemoveCookieDto, Space } from "$lib/bindings";
    import { toast } from "svelte-sonner";
    import { Checkbox } from "$lib/components/primitives/checkbox";
    import { Label } from "$lib/components/primitives/label";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
    };

    let { pane, isCollapsed = $bindable() }: Props = $props();

    let spaceSettingsStr: string = $state(
        zakuState.activeSpace ? JSON.stringify(zakuState.activeSpace.settings) : String(),
    );

    let shouldRenderCreateNewRequestInput = $derived(
        treeActionsState.createNewItem === "request" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(RELATIVE_SPACE_ROOT),
    );
    let shouldRenderCreateNewCollectionInput = $derived(
        treeActionsState.createNewItem === "collection" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(RELATIVE_SPACE_ROOT),
    );
</script>

{#snippet cookies(activeSpaceRef: Space)}
    <div class="flex h-full max-h-[calc(100%-1.5rem)] flex-col overflow-y-auto">
        {#if Object.keys(activeSpaceRef.cookies).length > 0}
            <Accordion type="multiple" class="bg-card/75 rounded-sm border">
                {#each Object.entries(activeSpaceRef.cookies) as [domain, cookies] (domain)}
                    <AccordionItem value={domain}>
                        <AccordionTrigger class="cursor-pointer px-3 hover:decoration-0">
                            {domain}
                        </AccordionTrigger>
                        <AccordionContent class="bg-background px-3 py-4">
                            {#if cookies}
                                <div class="flex gap-1.5">
                                    {#each cookies as ck, idx (idx)}
                                        <Badge variant="outline" class="p-1">
                                            <span class="px-2 select-text">{ck.name}</span>
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                class="size-4 max-h-4 min-h-4 max-w-4 min-w-4 cursor-pointer rounded-sm"
                                                onclick={async () => {
                                                    const removeCookieDto: RemoveCookieDto = {
                                                        domain: ck.domain,
                                                        path: ck.path,
                                                        name: ck.name,
                                                    };
                                                    const isRemoved = await commands.removeCookie(
                                                        activeSpaceRef.abspath,
                                                        removeCookieDto,
                                                    );

                                                    if (isRemoved) {
                                                        cookies.splice(idx, 1);

                                                        const domainCookies =
                                                            activeSpaceRef.cookies[domain];
                                                        if (
                                                            !domainCookies ||
                                                            domainCookies.length === 0
                                                        ) {
                                                            delete activeSpaceRef.cookies[domain];
                                                        }
                                                    } else {
                                                        toast(
                                                            `Unable to remove '${ck.name}' cookie`,
                                                        );
                                                    }
                                                }}
                                            >
                                                <XIcon class="size-1" size={4} />
                                                <span class="sr-only">Close</span>
                                            </Button>
                                        </Badge>
                                    {/each}
                                </div>
                            {/if}
                        </AccordionContent>
                    </AccordionItem>
                {/each}
            </Accordion>
        {/if}
    </div>
{/snippet}

{#if zakuState.activeSpace}
    {@const spaceRef = zakuState.activeSpace}
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
                    <TreeItemRoot currentPath={RELATIVE_SPACE_ROOT} root={spaceRef.root}>
                        {#if shouldRenderCreateNewRequestInput}
                            <TreeItemCreate
                                type={TreeItemType.Request}
                                parentRelativePath={RELATIVE_SPACE_ROOT}
                                level={1}
                            />
                        {/if}
                        {#each spaceRef.root.requests as request (request.meta.file_name)}
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
                        {#each spaceRef.root.collections as collection (collection.meta.dir_name)}
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
            <Dialog>
                <DialogTrigger>
                    <Button size="icon" variant="ghost">
                        <SettingsIcon strokeWidth={1.25} size={16} />
                        <span class="sr-only">Settings</span>
                    </Button>
                </DialogTrigger>
                <DialogContent class="flex h-[80%] max-h-[80%] w-[80%] max-w-[80%] flex-col">
                    <DialogHeader>
                        <DialogTitle>Settings</DialogTitle>
                        <DialogDescription>Manage space settings</DialogDescription>
                    </DialogHeader>
                    <div class="flex h-full max-h-[calc(100%-1.5rem)] flex-col overflow-y-auto">
                        <h3 class="text-medium mb-3 leading-none font-semibold tracking-tight">
                            Notifications
                        </h3>
                        <div class="flex items-center gap-1.5">
                            <Checkbox
                                id="settings.notifications.audio.on_req_finish"
                                bind:checked={spaceRef.settings.notifications.audio.on_req_finish}
                            />
                            <Label for="settings.notifications.audio.on_req_finish">
                                Play sound when a request finishes
                            </Label>
                        </div>
                    </div>
                    <DialogFooter>
                        <Button
                            disabled={spaceSettingsStr === JSON.stringify(spaceRef.settings)}
                            onclick={async () => {
                                await commands.updateSpaceSettings(
                                    spaceRef.abspath,
                                    spaceRef.settings,
                                );

                                spaceSettingsStr = JSON.stringify(spaceRef.settings);
                                toast.success(`Updated space settings`);
                            }}
                        >
                            Save
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
            <Dialog>
                <DialogTrigger>
                    <Button size="icon" variant="ghost">
                        <CookieIcon size={14} />
                    </Button>
                </DialogTrigger>
                <DialogContent class="flex h-[80%] max-h-[80%] w-[80%] max-w-[80%] flex-col">
                    <DialogHeader>
                        <DialogTitle>Cookies</DialogTitle>
                        <DialogDescription>Manage space cookies</DialogDescription>
                    </DialogHeader>
                    <div class="flex h-full max-h-[calc(100%-1.5rem)] flex-col overflow-y-auto">
                        {@render cookies(spaceRef)}
                    </div>
                </DialogContent>
            </Dialog>
        </div>
    </div>
{/if}
