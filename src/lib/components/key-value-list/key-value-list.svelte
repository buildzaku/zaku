<script lang="ts">
    import { Plus, Trash2 } from "lucide-svelte";

    import { Input } from "$lib/components/primitives/input";
    import { Button } from "$lib/components/primitives/button";
    import { Checkbox } from "$lib/components/primitives/checkbox";
    import type { KeyValuePair } from "$lib/utils/api";

    export let type: "parameter" | "header";
    export let pairs: KeyValuePair[] = [];

    function addPair() {
        pairs = [...pairs, { key: "", value: "", include: true }];
    }

    function deletePairAt(index: number) {
        pairs = pairs.filter((_, idx) => idx !== index);
    }
</script>

<div class={`flex flex-col gap-2 ${$$props["class"]}`}>
    {#each pairs as pair, index}
        <div class="flex gap-2">
            <div class="flex size-7 items-center justify-center">
                <Checkbox
                    checked={pair.include}
                    onCheckedChange={() => {
                        pair.include = !pair.include;
                    }}
                />
            </div>
            <Input
                type="text"
                disabled={!pair.include}
                bind:value={pair.key}
                placeholder="Key"
                class="font-mono text-xs"
            />
            <Input
                type="text"
                disabled={!pair.include}
                bind:value={pair.value}
                placeholder="Value"
                class="font-mono text-xs"
            />
            <Button
                variant="outline"
                class="bg-transparent p-[7px] hover:bg-muted/40 hover:text-destructive"
                on:click={() => deletePairAt(index)}
            >
                <Trash2 size={14} />
            </Button>
        </div>
    {/each}
    <div>
        <Button variant="ghost" on:click={addPair} class="h-7 gap-1 border px-2">
            <Plus size={14} />
            <span class="text-small">
                Add {type.replace(/^(.)/, match => match.toUpperCase())}
            </span>
        </Button>
    </div>
</div>
