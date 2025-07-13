<script lang="ts">
    import { PlusIcon, Trash2Icon } from "@lucide/svelte";

    import { Input } from "$lib/components/primitives/input";
    import { Button } from "$lib/components/primitives/button";
    import { Checkbox } from "$lib/components/primitives/checkbox";
    import { baseRequestHeaders } from "$lib/state.svelte";
    import type { ReqCfg } from "$lib/bindings";

    type Props = {
        config: ReqCfg;
    };

    let { config = $bindable() }: Props = $props();

    function addHeader(config: ReqCfg) {
        if (!config.headers) {
            config.headers = [];
        }

        config.headers.push([true, "", ""]);
    }

    function deleteHeader(config: ReqCfg, index: number) {
        if (!config.headers) return;
        config.headers = config.headers.filter((_, idx) => idx !== index);
    }
</script>

<div class="flex flex-col gap-3">
    {#each baseRequestHeaders as baseHeader, idx (idx)}
        <div class="flex gap-2">
            <div class="flex size-6 items-center justify-center">
                <Checkbox checked={true} disabled={true} />
            </div>
            <Input
                type="text"
                disabled={!baseHeader[0]}
                bind:value={baseHeader[1]}
                placeholder="Key"
                class="font-mono text-xs"
            />
            <Input
                type="text"
                disabled={!baseHeader[0]}
                bind:value={baseHeader[2]}
                placeholder="Value"
                class="font-mono text-xs"
            />
            <Button
                disabled={true}
                variant="outline"
                class="hover:bg-muted/40 hover:text-destructive bg-transparent p-[7px]"
            >
                <Trash2Icon size={14} class="max-h-[14px] max-w-[14px]" />
            </Button>
        </div>
    {/each}
    {#each config.headers ?? [] as pair, idx (idx)}
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
                class="hover:bg-muted/40 hover:text-destructive bg-transparent p-[7px]"
                onclick={() => {
                    deleteHeader(config, idx);
                }}
            >
                <Trash2Icon size={14} class="max-h-[14px] max-w-[14px]" />
            </Button>
        </div>
    {/each}
    <Button variant="ghost" onclick={() => addHeader(config)} class="h-6 w-fit gap-1 border px-2">
        <PlusIcon size={14} class="max-h-[14px] max-w-[14px]" />
        <span class="text-small">Add Header</span>
    </Button>
</div>
