<script lang="ts">
  import { METHODS } from "$lib/utils/constants";
  import {
    Select,
    SelectTrigger,
    SelectContent,
    SelectItem,
  } from "$lib/components/primitives/select";
  import { requestColors } from "$lib/utils/style";

  let { selected = $bindable() }: { selected: string } = $props();
</script>

<Select type="single" bind:value={selected}>
  <SelectTrigger class="w-32 font-medium">
    <span class={requestColors({ method: selected })}>{selected}</span>
  </SelectTrigger>
  <SelectContent class="w-[105px] font-medium">
    {#each Object.values(METHODS) as METHOD (METHOD)}
      <SelectItem value={METHOD} class={requestColors({ method: METHOD })}>
        {METHOD}
      </SelectItem>
    {/each}
  </SelectContent>
</Select>
