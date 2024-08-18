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
    import type { WorkspaceStoreDto } from "$lib/store";
    import { FolderIcon } from "lucide-svelte";

    export let isCollapsed: boolean;
    export let activeWorkspace: WorkspaceStoreDto;
    export let workspaces: WorkspaceStoreDto[];

    // onSelectedChange={e => {
    //     selectedAccount = workspaces.find(account => account.email === e?.value) || workspaces[0];
    // }}
</script>

<div class="h-7 w-full">
    <Select portal={null} selected={{ value: activeWorkspace.path, label: activeWorkspace.name }}>
        <SelectTrigger
            class={cn(
                "flex h-7 w-full items-center gap-2 bg-muted/40 hover:bg-muted/60",
                isCollapsed && "flex size-7 items-center justify-center p-0",
            )}
            withCaret={!isCollapsed}
            aria-label="Select workspace"
        >
            <div class="pointer-events-none flex items-center gap-2 overflow-hidden">
                <div>
                    <FolderIcon size={14} />
                </div>
                {#if !isCollapsed}
                    <span class="truncate">{activeWorkspace.name}</span>
                {/if}
            </div>
        </SelectTrigger>
        <SelectContent sameWidth={!isCollapsed} align={isCollapsed ? "start" : undefined}>
            <SelectGroup>
                {#each workspaces as workspace}
                    <SelectItem value={workspace.path} label={workspace.name}>
                        <div class="flex items-center gap-2 overflow-hidden">
                            <div>
                                <FolderIcon size={14} />
                            </div>
                            <span class="truncate">{workspace.name}</span>
                        </div>
                    </SelectItem>
                {/each}
            </SelectGroup>
        </SelectContent>
        <SelectInput hidden name="workspace" />
    </Select>
</div>
