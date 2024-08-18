<script lang="ts">
    import { activeWorkspace, getPersistedStore } from "$lib/store";
    import { Button } from "$lib/components/primitives/button";
    import {
        DropdownMenu,
        DropdownMenuContent,
        DropdownMenuItem,
        DropdownMenuSeparator,
        DropdownMenuTrigger,
    } from "$lib/components/primitives/dropdown-menu";

    import { CookieIcon, SettingsIcon, Trash2Icon } from "lucide-svelte";
    import WorkspaceSwitcher from "../workspace-switcher/workspace-switcher.svelte";
    import { cn } from "$lib/utils/style";

    async function handleDelete() {
        console.log("deleting workspace... pepeoeo");

        await activeWorkspace.delete();
    }

    export let isCollapsed = false;
</script>

{#if $activeWorkspace}
    <div class="flex size-full flex-col justify-between">
        <div class="flex w-full items-center justify-between border-b p-1.5">
            <WorkspaceSwitcher
                activeWorkspace={{
                    name: $activeWorkspace.config.name,
                    path: $activeWorkspace.path,
                }}
                {isCollapsed}
                workspaces={[
                    {
                        name: $activeWorkspace.config.name,
                        path: $activeWorkspace.path,
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
                    <DropdownMenuItem on:click={handleDelete}>
                        <div class="flex items-center gap-1.5 text-small text-destructive">
                            <Trash2Icon strokeWidth={2.25} size={13} />
                            <span>Delete workspace</span>
                        </div>
                    </DropdownMenuItem>
                </DropdownMenuContent>
            </DropdownMenu>
            <Button size="icon" variant="ghost-hover">
                <CookieIcon size={14} />
            </Button>
        </div>
    </div>
{/if}
