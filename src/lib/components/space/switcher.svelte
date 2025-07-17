<script lang="ts">
    import { goto } from "$app/navigation";
    import { ChevronDownIcon, CheckIcon, PyramidIcon } from "@lucide/svelte";

    import { treeActionsState, treeNodesState, sharedState } from "$lib/state.svelte";
    import { buttonVariants } from "$lib/components/primitives/button";
    import {
        DropdownMenu,
        DropdownMenuContent,
        DropdownMenuItem,
        DropdownMenuSub,
        DropdownMenuTrigger,
        DropdownMenuSubTrigger,
        DropdownMenuSubContent,
        DropdownMenuSeparator,
    } from "$lib/components/primitives/dropdown-menu";
    import { cn } from "$lib/utils/style";
    import { SpaceCreateDialog } from ".";
    import { commands } from "$lib/bindings";

    type Props = { isSidebarCollapsed: boolean };

    let { isSidebarCollapsed }: Props = $props();

    let isCreateSpaceDialogOpen = $state(false);

    async function handleOpenExistingSpace() {
        try {
            const cmdResult = await commands.openDirDialog({ title: "Open an existing Space" });
            if (cmdResult.status === "error") {
                throw new Error("Unable to open existing space");
            }
            if (!cmdResult.data) {
                return;
            }

            const spaceRefCmdResult = await commands.getSpaceref(cmdResult.data);
            if (spaceRefCmdResult.status === "error") {
                throw new Error(`Cannot get space reference for ${cmdResult.data}`);
            }

            await sharedState.setActiveSpace(spaceRefCmdResult.data);
            await goto("/space");
        } catch (err) {
            console.error(err);
            await commands.dispatchNotif({
                title: "Doesn't look like a valid space.",
                body: "Unable to parse the directory, make sure it is a valid space and try again.",
            });
        }
    }

    async function handleDeleteSpace() {
        if (sharedState.activeSpace) {
            try {
                const spaceRefCmdResult = await commands.getSpaceref(
                    sharedState.activeSpace.abspath,
                );
                if (spaceRefCmdResult.status === "error") {
                    throw new Error(
                        `Cannot get space reference for ${sharedState.activeSpace.abspath}`,
                    );
                }

                await commands.removeSpace(spaceRefCmdResult.data);
                await sharedState.synchronize();

                return;
            } catch (err) {
                console.error(err);
            }
        }
    }
</script>

{#if sharedState.activeSpace}
    <DropdownMenu>
        <DropdownMenuTrigger
            class={cn(
                buttonVariants({
                    variant: "outline",
                    size: isSidebarCollapsed ? "icon" : "default",
                }),
                "flex h-7 w-full items-center justify-center",
                isSidebarCollapsed ? "my-0.5 size-6" : "justify-start gap-2 px-1.5",
            )}
        >
            <PyramidIcon size={14} class="max-h-[14px] max-w-[14px]" />
            {#if !isSidebarCollapsed}
                <div class="flex min-w-0 grow items-center justify-between">
                    <span
                        class="min-w-0 truncate overflow-hidden pr-0.5 text-ellipsis whitespace-nowrap"
                    >
                        {sharedState.activeSpace.meta.name}
                    </span>
                    <ChevronDownIcon size={14} class="max-h-[14px] max-w-[14px]" />
                </div>
            {/if}
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" side="right" class="w-[224px]">
            <DropdownMenuItem class="text-small h-7 rounded-md px-2" disabled>
                Space settings
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuSub>
                <DropdownMenuSubTrigger class="text-small h-7 rounded-md px-2">
                    <p>Switch Space</p>
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent class="w-[185px]" sideOffset={4}>
                    {#each sharedState.spaceRefs as spaceRef (spaceRef.path)}
                        <DropdownMenuItem
                            class="text-small flex h-7 justify-between rounded-md px-2"
                            onclick={async () => {
                                await sharedState.setActiveSpace(spaceRef);
                                treeActionsState.reset();
                                treeNodesState.reset();
                            }}
                        >
                            <div class="flex items-center overflow-hidden">
                                <span class="truncate">{spaceRef.name}</span>
                            </div>
                            {#if spaceRef.path === sharedState.activeSpace.abspath}
                                <CheckIcon size={14} class="max-h-[14px] max-w-[14px]" />
                            {/if}
                        </DropdownMenuItem>
                    {/each}
                </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuSeparator />
            <DropdownMenuItem
                class="text-small h-7 rounded-md px-2"
                onclick={() => {
                    isCreateSpaceDialogOpen = true;
                }}
            >
                <span>Create a new Space</span>
            </DropdownMenuItem>
            <DropdownMenuItem
                class="text-small h-7 rounded-md px-2"
                onclick={handleOpenExistingSpace}
            >
                <span>Open an existing Space</span>
            </DropdownMenuItem>
            <DropdownMenuItem class="text-small h-7 rounded-md px-2" onclick={handleDeleteSpace}>
                <span class="text-destructive">Delete space</span>
            </DropdownMenuItem>
        </DropdownMenuContent>
    </DropdownMenu>
    <SpaceCreateDialog bind:isOpen={isCreateSpaceDialogOpen} />
{/if}
