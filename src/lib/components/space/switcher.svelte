<script lang="ts">
    import {
        DropdownMenu,
        DropdownMenuContent,
        DropdownMenuItem,
        DropdownMenuSub,
        DropdownMenuTrigger,
    } from "$lib/components/primitives/dropdown-menu";
    import { ChevronDownIcon, CheckIcon, PyramidIcon } from "lucide-svelte";

    import { zakuState } from "$lib/store";
    import { Button } from "../primitives/button";
    import DropdownMenuSeparator from "../primitives/dropdown-menu/dropdown-menu-separator.svelte";
    import DropdownMenuSubTrigger from "../primitives/dropdown-menu/dropdown-menu-sub-trigger.svelte";
    import DropdownMenuSubContent from "../primitives/dropdown-menu/dropdown-menu-sub-content.svelte";
    import { goto } from "$app/navigation";
    import { openDirectoryDialog, getSpaceReference, dispatchNotification } from "$lib/commands";
    import { SpaceCreateDialog } from ".";
    import { cn } from "$lib/utils/style";

    export let isSidebarCollapsed: boolean;

    let isCreateSpaceDialogOpen = false;

    async function handleOpenExistingSpace() {
        try {
            const selectedPath = await openDirectoryDialog({ title: "Open an existing Space" });

            if (selectedPath !== null) {
                const spaceReference = await getSpaceReference(selectedPath);
                await zakuState.setActiveSpace(spaceReference);
                await goto("/space");
            }
        } catch (err) {
            console.error(err);
            await dispatchNotification({
                title: "Doesn't look like a valid space.",
                body: "Unable to parse the directory, make sure it is a valid space and try again.",
            });
        }
    }

    async function handleDelete() {
        if ($zakuState.active_space) {
            await zakuState.deleteSpace($zakuState.active_space.path);
        }
    }
</script>

{#if $zakuState.active_space}
    <DropdownMenu>
        <DropdownMenuTrigger asChild let:builder>
            <Button
                builders={[builder]}
                variant="outline"
                size={isSidebarCollapsed ? "icon" : "default"}
                class={cn(
                    "flex h-7 w-full items-center justify-center",
                    isSidebarCollapsed ? "my-0.5 size-6" : "justify-start gap-2 px-1.5",
                )}
            >
                <PyramidIcon size={14} class="min-h-[14px] min-w-[14px]" />
                {#if !isSidebarCollapsed}
                    <div class="flex grow items-center justify-between overflow-hidden">
                        <span class="truncate pr-0.5"
                            >{$zakuState.active_space.config.meta.name}</span
                        >
                        <ChevronDownIcon size={14} class="min-h-[14px] min-w-[14px]" />
                    </div>
                {/if}
            </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" side="right" class="w-[224px]">
            <DropdownMenuItem class="h-8 rounded-md px-2 text-small" disabled>
                Space settings
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuSub>
                <DropdownMenuSubTrigger class="h-8 rounded-md px-2 text-small">
                    <p>Switch Space</p>
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent class="w-[185px]" sideOffset={4}>
                    {#each $zakuState.space_references as spaceReference}
                        <DropdownMenuItem
                            class="flex h-8 justify-between rounded-md px-2 text-small"
                            on:click={async () => {
                                await zakuState.setActiveSpace(spaceReference);
                            }}
                        >
                            <div class="flex items-center overflow-hidden">
                                <span class="truncate">{spaceReference.name}</span>
                            </div>
                            {#if spaceReference.path === $zakuState.active_space.path}
                                <CheckIcon size={14} class="min-h-[14px] min-w-[14px]" />
                            {/if}
                        </DropdownMenuItem>
                    {/each}
                </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuSeparator />
            <DropdownMenuItem
                class="h-8 rounded-md px-2 text-small"
                on:click={() => {
                    isCreateSpaceDialogOpen = true;
                }}
            >
                <span>Create a new Space</span>
            </DropdownMenuItem>
            <DropdownMenuItem
                class="h-8 rounded-md px-2 text-small"
                on:click={handleOpenExistingSpace}
            >
                <span>Open an existing Space</span>
            </DropdownMenuItem>
            <DropdownMenuItem class="h-8 rounded-md px-2 text-small" on:click={handleDelete}>
                <span class="text-destructive">Delete space</span>
            </DropdownMenuItem>
        </DropdownMenuContent>
    </DropdownMenu>
    <SpaceCreateDialog
        bind:isOpen={isCreateSpaceDialogOpen}
        onCreate={async () => {
            isCreateSpaceDialogOpen = false;
        }}
    />
{/if}
