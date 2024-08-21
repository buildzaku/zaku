<script lang="ts">
    import { goto } from "$app/navigation";
    import { activeSpace } from "$lib/store";
    import { Button } from "$lib/components/primitives/button";
    import {
        DropdownMenu,
        DropdownMenuContent,
        DropdownMenuItem,
        DropdownMenuTrigger,
    } from "$lib/components/primitives/dropdown-menu";

    import { CookieIcon, SettingsIcon, Trash2Icon, PlusIcon } from "lucide-svelte";
    import { SpaceCreateDialog, SpaceSwitcher } from "$lib/components/space";
    import { cn } from "$lib/utils/style";
    import { dispatchNotification, openDirectoryDialog } from "$lib/commands";

    export let isCollapsed = false;

    let isCreateSpaceDialogOpen = false;

    async function handleOpenExistingSpace() {
        try {
            const selectedPath = await openDirectoryDialog({ title: "Open an existing Space" });

            if (selectedPath !== null) {
                await activeSpace.set(selectedPath);
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
        await activeSpace.delete();
        await goto("/");
    }
</script>

{#if $activeSpace}
    <div class="flex size-full flex-col justify-between">
        <div class="flex w-full items-center justify-center border-b p-1.5">
            <SpaceSwitcher
                activeSpace={{
                    name: $activeSpace.config.meta.name,
                    path: $activeSpace.path,
                }}
                {isCollapsed}
                spaces={[
                    {
                        name: $activeSpace.config.meta.name,
                        path: $activeSpace.path,
                    },
                ]}
            />
        </div>
        <div class="flex-grow overflow-y-auto p-1.5">
            {#if !isCollapsed}
                <!-- TODO - file tree -->

                <Button variant="ghost-hover" class="w-full">+ New Request</Button>
            {/if}
        </div>
        <div
            class={cn(
                "flex items-center justify-between gap-1.5 border-t p-1.5",
                isCollapsed && "flex-col-reverse",
            )}
        >
            <DropdownMenu>
                <DropdownMenuTrigger asChild let:builder>
                    <Button builders={[builder]} size="icon" variant="ghost-hover">
                        <SettingsIcon strokeWidth={1.25} size={16} />
                        <span class="sr-only">Settings</span>
                    </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                    <DropdownMenuItem
                        on:click={() => {
                            isCreateSpaceDialogOpen = true;
                        }}
                    >
                        <div class="flex items-center gap-1.5 text-small">
                            <PlusIcon strokeWidth={2.25} size={13} />
                            <span>Create new Space</span>
                        </div>
                    </DropdownMenuItem>
                    <DropdownMenuItem on:click={handleOpenExistingSpace}>
                        <div class="flex items-center gap-1.5 text-small">
                            <PlusIcon strokeWidth={2.25} size={13} />
                            <span>Open existing Space</span>
                        </div>
                    </DropdownMenuItem>
                    <DropdownMenuItem on:click={handleDelete}>
                        <div class="flex items-center gap-1.5 text-small text-destructive">
                            <Trash2Icon strokeWidth={2.25} size={13} />
                            <span>Delete space</span>
                        </div>
                    </DropdownMenuItem>
                </DropdownMenuContent>
            </DropdownMenu>
            <Button size="icon" variant="ghost-hover">
                <CookieIcon size={14} />
            </Button>
        </div>
    </div>
    <SpaceCreateDialog
        bind:isOpen={isCreateSpaceDialogOpen}
        onCreate={async () => {
            isCreateSpaceDialogOpen = false;
        }}
    />
{/if}
