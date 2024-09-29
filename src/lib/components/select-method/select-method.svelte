<script lang="ts">
    import { METHODS } from "$lib/utils/constants";
    import {
        Select,
        SelectTrigger,
        SelectValue,
        SelectContent,
        SelectGroup,
        SelectItem,
        SelectInput,
    } from "$lib/components/primitives/select";
    import { cn, getMethodColorClass } from "$lib/utils/style";

    export let selected: (typeof METHODS)[keyof typeof METHODS];
</script>

<Select bind:selected>
    <SelectTrigger class={cn("w-32", getMethodColorClass(selected.value))}>
        <SelectValue placeholder="HTTP method" />
    </SelectTrigger>
    <SelectContent>
        <SelectGroup class="flex flex-col gap-1">
            {#each Object.values(METHODS) as METHOD}
                <SelectItem
                    value={METHOD.value}
                    label={METHOD.label}
                    class={getMethodColorClass(METHOD.value)}
                >
                    {METHOD.label}
                </SelectItem>
            {/each}
        </SelectGroup>
    </SelectContent>
    <SelectInput name="http-method" class={getMethodColorClass(selected.value)} />
</Select>
