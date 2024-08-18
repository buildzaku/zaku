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
    import type { SpaceStoreDto } from "$lib/store";
    import { FolderIcon } from "lucide-svelte";

    export let isCollapsed: boolean;
    export let activeSpace: SpaceStoreDto;
    export let spaces: SpaceStoreDto[];

    // onSelectedChange={e => {
    //     selectedAccount = spaces.find(account => account.email === e?.value) || spaces[0];
    // }}
</script>

<div class="h-6 w-full">
    <Select portal={null} selected={{ value: activeSpace.path, label: activeSpace.name }}>
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
                    <span class="truncate">{activeSpace.name}</span>
                {/if}
            </div>
        </SelectTrigger>
        <SelectContent sameWidth={!isCollapsed} align={isCollapsed ? "start" : undefined}>
            <SelectGroup>
                {#each spaces as space}
                    <SelectItem value={space.path} label={space.name}>
                        <div class="flex items-center gap-2 overflow-hidden">
                            <div>
                                <FolderIcon size={14} />
                            </div>
                            <span class="truncate">{space.name}</span>
                        </div>
                    </SelectItem>
                {/each}
            </SelectGroup>
        </SelectContent>
        <SelectInput hidden name="space" />
    </Select>
</div>
