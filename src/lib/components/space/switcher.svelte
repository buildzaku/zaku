<script lang="ts">
    import { cn } from "$lib/utils/style";
    import {
        Select,
        SelectContent,
        SelectGroup,
        SelectInput,
        SelectItem,
        SelectTrigger,
    } from "$lib/components/primitives/select";
    import { FolderIcon } from "lucide-svelte";
    import { zakuState } from "$lib/store";

    export let isCollapsed: boolean;
</script>

{#if $zakuState.active_space}
    <div class="h-6 w-full">
        <Select
            portal={null}
            selected={{
                value: $zakuState.active_space.path,
                label: $zakuState.active_space.config.meta.name,
            }}
        >
            <SelectTrigger
                class={cn(
                    "flex h-6 w-full items-center gap-2 bg-muted/40 hover:bg-muted/60",
                    isCollapsed && "flex size-6 items-center justify-center p-0",
                )}
                withCaret={!isCollapsed}
                aria-label="Select space"
            >
                <div class="pointer-events-none flex items-center gap-2 overflow-hidden">
                    <div>
                        <FolderIcon size={14} />
                    </div>
                    {#if !isCollapsed}
                        <span class="truncate">{$zakuState.active_space.config.meta.name}</span>
                    {/if}
                </div>
            </SelectTrigger>
            <SelectContent sameWidth={!isCollapsed} align={isCollapsed ? "start" : undefined}>
                {#each $zakuState.space_references as spaceReference}
                    <SelectItem
                        value={spaceReference.path}
                        label={spaceReference.name}
                        on:click={async () => {
                            await zakuState.set(spaceReference);
                        }}
                    >
                        <div class="flex items-center gap-2 overflow-hidden">
                            <div>
                                <FolderIcon size={14} />
                            </div>
                            <span class="truncate">{spaceReference.name}</span>
                        </div>
                    </SelectItem>
                {/each}
            </SelectContent>
            <SelectInput hidden name="space" />
        </Select>
    </div>
{/if}
