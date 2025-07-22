<script lang="ts">
    import { sharedState, explorerActionsState } from "$lib/state.svelte";
    import { Button, buttonVariants } from "$lib/components/primitives/button";
    import { CookieIcon, SettingsIcon, ChevronsLeftIcon, CompassIcon, XIcon } from "@lucide/svelte";
    import type { PaneAPI } from "paneforge";

    import { SpaceSwitcher } from "$lib/components/space";
    import { cn } from "$lib/utils/style";
    import { TreeNodeContent, TreeNodeCreate, TreeNodeRoot } from "$lib/components/tree-node";
    import {
        Tooltip,
        TooltipTrigger,
        TooltipContent,
        TooltipProvider,
    } from "$lib/components/primitives/tooltip";
    import { RELATIVE_SPACE_ROOT } from "$lib/utils/constants";
    import { isCurrentCollectionOrAnyOfItsChildFocussed } from "$lib/components/tree-node/utils.svelte";
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
    import type { RemoveCookieDto, Space, SpaceCookie } from "$lib/bindings";
    import { toast } from "svelte-sonner";
    import { Checkbox } from "$lib/components/primitives/checkbox";
    import { Label } from "$lib/components/primitives/label";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
    };

    let { pane, isCollapsed = $bindable() }: Props = $props();

    let spaceSettingsStr: string = $state(
        sharedState.space ? JSON.stringify(sharedState.space.settings) : String(),
    );

    let shouldRenderCreateNewRequestInput = $derived(
        explorerActionsState.createNewNode === "request" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(RELATIVE_SPACE_ROOT),
    );
    let shouldRenderCreateNewCollectionInput = $derived(
        explorerActionsState.createNewNode === "collection" &&
            isCurrentCollectionOrAnyOfItsChildFocussed(RELATIVE_SPACE_ROOT),
    );
</script>

{#snippet settingsButton(space: Space)}
    <Dialog>
        <DialogTrigger
            class={buttonVariants({
                variant: "ghost",
                size: "icon",
            })}
        >
            <SettingsIcon strokeWidth={1.25} size={16} />
            <span class="sr-only">Settings</span>
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
                        bind:checked={space.settings.notifications.audio.on_req_finish}
                    />
                    <Label for="settings.notifications.audio.on_req_finish">
                        Play sound when a request finishes
                    </Label>
                </div>
            </div>
            <DialogFooter>
                <Button
                    disabled={spaceSettingsStr === JSON.stringify(space.settings)}
                    onclick={async () => {
                        const saveResult = await commands.saveSpaceSettings(
                            space.abspath,
                            space.settings,
                        );
                        if (saveResult.status !== "ok") {
                            const { kind, details, message } = saveResult.error;
                            console.error([kind, details].join(" - "));
                            toast.error(message);
                            return;
                        }

                        spaceSettingsStr = JSON.stringify(space.settings);
                        toast.success(`Changes saved to space settings`);
                    }}
                >
                    Save
                </Button>
            </DialogFooter>
        </DialogContent>
    </Dialog>
{/snippet}

{#snippet cookiesContent(space: Space, domain: string, cookies: SpaceCookie[])}
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
                            space.abspath,
                            removeCookieDto,
                        );

                        if (isRemoved) {
                            cookies.splice(idx, 1);

                            const domainCookies = space.cookies[domain];
                            if (!domainCookies || domainCookies.length === 0) {
                                delete space.cookies[domain];
                            }
                        } else {
                            toast.error(`Unable to remove '${ck.name}' cookie`);
                        }
                    }}
                >
                    <XIcon class="size-1" size={4} />
                    <span class="sr-only">Close</span>
                </Button>
            </Badge>
        {/each}
    </div>
{/snippet}

{#snippet cookiesButton(space: Space)}
    <Dialog>
        <DialogTrigger class={buttonVariants({ variant: "ghost", size: "icon" })}>
            <CookieIcon size={14} />
        </DialogTrigger>
        <DialogContent class="flex h-[80%] max-h-[80%] w-[80%] max-w-[80%] flex-col">
            <DialogHeader>
                <DialogTitle>Cookies</DialogTitle>
                <DialogDescription>Manage space cookies</DialogDescription>
            </DialogHeader>
            <div class="flex h-full max-h-[calc(100%-1.5rem)] flex-col overflow-y-auto">
                {#if Object.keys(space.cookies).length > 0}
                    <Accordion type="multiple" class="bg-card/75 rounded-sm border">
                        {#each Object.entries(space.cookies) as [domain, cookies] (domain)}
                            <AccordionItem value={domain}>
                                <AccordionTrigger class="cursor-pointer px-3 hover:decoration-0">
                                    {domain}
                                </AccordionTrigger>
                                <AccordionContent class="bg-background border-t-[1px] px-3 py-4">
                                    {#if cookies}
                                        {@render cookiesContent(space, domain, cookies)}
                                    {/if}
                                </AccordionContent>
                            </AccordionItem>
                        {/each}
                    </Accordion>
                {/if}
            </div>
        </DialogContent>
    </Dialog>
{/snippet}

{#if sharedState.space}
    {@const spaceSnapshot = sharedState.space}
    <div class="flex size-full flex-col justify-between">
        <!-- align-marker: matches ResizablePane's mt-px -->
        <div class="flex w-full items-center justify-center gap-1.5 border-b p-1.5 pt-px">
            <div class="flex min-w-0 grow">
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
        <div class="group/explorer flex w-full grow items-start justify-center overflow-y-auto">
            {#if isCollapsed}
                <TooltipProvider>
                    <Tooltip delayDuration={500} disableHoverableContent>
                        <TooltipTrigger
                            class={cn(
                                buttonVariants({
                                    variant: "ghost",
                                    size: "icon",
                                }),
                                "my-1.5 flex-shrink-0",
                            )}
                            onclick={() => {
                                pane.expand();
                                pane.resize(24);
                            }}
                        >
                            <CompassIcon size={14} class="min-h-[14px] min-w-[14px]" />
                        </TooltipTrigger>
                        <TooltipContent side="right">Explorer</TooltipContent>
                    </Tooltip>
                </TooltipProvider>
            {:else}
                <div class="size-full">
                    <p class="text-muted-foreground flex h-[36px] items-center px-[22px]">
                        Explorer
                    </p>
                    <TreeNodeRoot
                        currentPath={RELATIVE_SPACE_ROOT}
                        root={spaceSnapshot.root_collection}
                    >
                        {#if shouldRenderCreateNewRequestInput}
                            <TreeNodeCreate
                                type="request"
                                parentRelativePath={RELATIVE_SPACE_ROOT}
                                level={1}
                            />
                        {/if}
                        {#each spaceSnapshot.root_collection.requests as request (request.meta.fsname)}
                            <TreeNodeContent
                                parentRelpath={RELATIVE_SPACE_ROOT}
                                parentNames={[]}
                                currentPath={request.meta.fsname}
                                node={request}
                                level={1}
                            />
                        {/each}

                        {#if shouldRenderCreateNewCollectionInput}
                            <TreeNodeCreate
                                type="collection"
                                parentRelativePath={RELATIVE_SPACE_ROOT}
                                level={1}
                            />
                        {/if}
                        {#each spaceSnapshot.root_collection.collections as collection (collection.meta.fsname)}
                            <TreeNodeContent
                                parentRelpath={RELATIVE_SPACE_ROOT}
                                parentNames={[]}
                                currentPath={collection.meta.fsname}
                                node={collection}
                                level={1}
                            />
                        {/each}
                    </TreeNodeRoot>
                </div>
            {/if}
        </div>

        {#if isCollapsed}
            <div class="flex flex-col items-center justify-between gap-1.5 border-t p-1.5">
                {@render cookiesButton(spaceSnapshot)}
                {@render settingsButton(spaceSnapshot)}
            </div>
        {:else}
            <div class="flex items-center justify-between gap-1.5 border-t p-1.5">
                {@render settingsButton(spaceSnapshot)}
                {@render cookiesButton(spaceSnapshot)}
            </div>
        {/if}
    </div>
{/if}
