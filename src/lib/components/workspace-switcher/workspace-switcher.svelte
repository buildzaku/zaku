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

<Select portal={null} selected={{ value: activeWorkspace.path, label: activeWorkspace.name }}>
    <SelectTrigger
        class={cn(
            "flex items-center gap-2",
            isCollapsed && "flex size-7 items-center justify-center p-0",
        )}
        withCaret={!isCollapsed}
        aria-label="Select workspace"
    >
        <div class="pointer-events-none flex items-center">
            <FolderIcon size={16} />
            <span class={cn(isCollapsed ? "!ml-0 !hidden" : "ml-2")}>
                {activeWorkspace.name}
            </span>
        </div>
    </SelectTrigger>
    <SelectContent sameWidth={!isCollapsed} align={isCollapsed ? "start" : undefined}>
        <SelectGroup>
            {#each workspaces as workspace}
                <SelectItem value={workspace.path} label={workspace.name}>
                    <div
                        class="flex items-center gap-3 [&_svg]:h-4 [&_svg]:w-4 [&_svg]:shrink-0 [&_svg]:text-foreground"
                    >
                        <FolderIcon size={16} />
                        {workspace.name}
                    </div>
                </SelectItem>
            {/each}
        </SelectGroup>
    </SelectContent>
    <SelectInput hidden name="account" />
</Select>
