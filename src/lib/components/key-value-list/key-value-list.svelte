<script lang="ts">
    import { PlusIcon, Trash2Icon } from "lucide-svelte";

    import { Input } from "$lib/components/primitives/input";
    import { Button } from "$lib/components/primitives/button";
    import { Checkbox } from "$lib/components/primitives/checkbox";
    import type { KeyValuePair } from "$lib/utils/api";
    import { cn } from "$lib/utils/style";

    type Props = {
        type: "parameter" | "header";
        pairs: [boolean, string, string][];
        class?: string;
    };

    let { type, pairs = $bindable(), class: className }: Props = $props();

    function addPair() {
        pairs.push([true, "", ""]);
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
                    checked={pair[0]}
                    onCheckedChange={() => {
                        pair[0] = !pair[0];
                    }}
                />
            </div>
            <Input
                type="text"
                disabled={!pair[0]}
                bind:value={pair[1]}
                placeholder="Key"
                class="font-mono text-xs"
            />
            <Input
                type="text"
                disabled={!pair[0]}
                bind:value={pair[2]}
                placeholder="Value"
                class="font-mono text-xs"
            />
            <Button
                variant="outline"
                class="bg-transparent p-[7px] hover:bg-muted/40 hover:text-destructive"
                onclick={() => deletePairAt(index)}
            >
                <Trash2Icon size={14} class="max-h-[14px] max-w-[14px]" />
            </Button>
        </div>
    {/each}
    <div>
        <Button variant="ghost" onclick={addPair} class="h-6 gap-1 border px-2">
            <PlusIcon size={14} class="max-h-[14px] max-w-[14px]" />
            <span class="text-small">
                Add {type.replace(/^(.)/, match => match.toUpperCase())}
            </span>
        </Button>
    </div>
</div>
