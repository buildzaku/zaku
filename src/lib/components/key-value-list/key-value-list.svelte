<script lang="ts">
    import { Plus, Trash2 } from "lucide-svelte";

    import { Input } from "$lib/components/primitives/input";
    import { Button } from "$lib/components/primitives/button";
    import { Checkbox } from "$lib/components/primitives/checkbox";
    import type { KeyValuePair } from "$lib/utils/api";
    import { cn } from "$lib/utils/style";

    type Props = {
        type: "parameter" | "header";
        pairs: KeyValuePair[];
        class?: string;
    };

    let { type, pairs = $bindable(), class: className }: Props = $props();

    // TODO - can just push with runes deep reactivity?
    function addPair() {
        pairs = [...pairs, { key: "", value: "", include: true }];
    }

    function deletePairAt(index: number) {
        pairs = pairs.filter((_, idx) => idx !== index);
    }
</script>

<div class={cn("flex flex-col gap-2", className)}>
    {#each pairs as pair, index}
        <div class="flex gap-2">
            <div class="flex size-6 items-center justify-center">
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
                onclick={() => deletePairAt(index)}
            >
                <Trash2 size={14} />
            </Button>
        </div>
    {/each}
    <div>
        <Button variant="ghost" onclick={addPair} class="h-6 gap-1 border px-2">
            <Plus size={14} />
            <span class="text-small">
                Add {type.replace(/^(.)/, match => match.toUpperCase())}
            </span>
        </Button>
    </div>
</div>
