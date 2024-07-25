<script lang="ts">
    import { METHOD_CLASS, METHODS } from "$lib/utils/constants";
    import {
        Select,
        SelectTrigger,
        SelectValue,
        SelectContent,
        SelectGroup,
        SelectItem,
        SelectInput,
    } from "../primitives/select";

    export let selected: (typeof METHODS)[keyof typeof METHODS];
</script>

<Select bind:selected>
    <SelectTrigger class={`w-32 ${METHOD_CLASS[selected.value]}`}>
        <SelectValue placeholder="HTTP method" />
    </SelectTrigger>
    <SelectContent>
        <SelectGroup class="flex flex-col gap-1">
            {#each Object.values(METHODS) as METHOD}
                <SelectItem
                    value={METHOD.value}
                    label={METHOD.label}
                    class={METHOD_CLASS[METHOD.value]}
                >
                    {METHOD.label}
                </SelectItem>
            {/each}
        </SelectGroup>
    </SelectContent>
    <SelectInput name="http-method" class={METHOD_CLASS[selected.value]} />
</Select>
