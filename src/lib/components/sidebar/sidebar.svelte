<script lang="ts">
    import { zakuState } from "$lib/store";
    import { Button } from "$lib/components/primitives/button";
    import type { PaneAPI } from "paneforge";

    import { CookieIcon, SettingsIcon, PanelLeftIcon } from "lucide-svelte";
    import { SpaceSwitcher } from "$lib/components/space";
    import { cn } from "$lib/utils/style";

    export let pane: PaneAPI;
    export let isCollapsed: boolean;
</script>

{#if $zakuState.active_space}
    <div class="flex size-full flex-col justify-between">
        <div class="mt-1.5 flex w-full items-center justify-center border-b p-1.5">
            <div
                class={cn(
                    "flex w-full items-center justify-between gap-1.5",
                    isCollapsed ? "flex-col" : "flex-row",
                )}
            >
                <div class="flex-grow overflow-hidden text-ellipsis whitespace-nowrap">
                    <SpaceSwitcher isSidebarCollapsed={isCollapsed} />
                </div>
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
                    <PanelLeftIcon size={14} class="min-h-[14px] min-w-[14px]" />
                </Button>
            </div>
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
