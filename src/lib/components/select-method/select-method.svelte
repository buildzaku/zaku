<script lang="ts">
    import { METHODS } from "$lib/utils/constants";
    import {
        Select,
        SelectTrigger,
        SelectContent,
        SelectItem,
    } from "$lib/components/primitives/select";
    import { getMethodColorClass } from "$lib/utils/style";

    let { selected = $bindable() }: { selected: (typeof METHODS)[keyof typeof METHODS] } = $props();
</script>

<Select type="single" bind:value={selected}>
    <SelectTrigger class="w-32">
        <span class={getMethodColorClass(selected)}>{selected}</span>
    </SelectTrigger>
    <SelectContent class="w-[105px]">
        {#each Object.values(METHODS) as METHOD}
            <SelectItem value={METHOD} class={getMethodColorClass(METHOD)}>
                {METHOD}
            </SelectItem>
        {/each}
    </SelectContent>
</Select>
