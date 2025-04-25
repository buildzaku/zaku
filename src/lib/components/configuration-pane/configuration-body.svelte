<script lang="ts">
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
    import {
        Select,
        SelectContent,
        SelectItem,
        SelectTrigger,
    } from "$lib/components/primitives/select";

    type Props = {
        selected: (typeof REQUEST_BODY_TYPES)[keyof typeof REQUEST_BODY_TYPES];
    };

    let { selected = $bindable() }: Props = $props();
</script>

<div class="flex h-9 items-center justify-start gap-3 border-b px-3">
    <span>Content Type</span>
    <Select type="single" bind:value={selected}>
        <SelectTrigger class="w-fit">
            <span class="pr-3">
                {selected}
            </span>
        </SelectTrigger>
        <SelectContent align="start">
            {#each Object.values(REQUEST_BODY_TYPES) as BODY_TYPE (BODY_TYPE)}
                <SelectItem value={BODY_TYPE}>
                    {BODY_TYPE}
                </SelectItem>
            {/each}
        </SelectContent>
    </Select>
</div>
