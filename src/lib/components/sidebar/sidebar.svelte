<script lang="ts">
    import { activeWorkspace, getPersistedStore } from "$lib/store";
    import { Button } from "$lib/components/primitives/button";

    async function handleDelete() {
        console.log("cleariing store");
        const persistedStore = await getPersistedStore();
        console.log(await persistedStore.entries());
        await persistedStore.clear();
        await persistedStore.save();
        await persistedStore.reset();
    }
</script>

<div class="flex flex-col items-center p-4">
    <Button on:click={handleDelete}>delete</Button>

    {#if $activeWorkspace}
        <span class="text inline-block select-text font-mono">{$activeWorkspace.config.name}</span>
    {:else}
        <span class="text inline-block select-text font-mono">Not found</span>
    {/if}
</div>
