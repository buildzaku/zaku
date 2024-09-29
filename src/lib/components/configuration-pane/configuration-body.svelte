<script lang="ts">
    import { tick } from "svelte";
    import DotFilled from "svelte-radix/DotFilled.svelte";
    import ChevronDown from "svelte-radix/ChevronDown.svelte";

    import { Command, CommandGroup, CommandItem } from "$lib/components/primitives/command";
    import { Popover, PopoverContent, PopoverTrigger } from "$lib/components/primitives/popover";
    import { Button } from "$lib/components/primitives/button";
    import { cn } from "$lib/utils/style";

    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";

    type RequestBody = (typeof REQUEST_BODY_TYPES)[keyof typeof REQUEST_BODY_TYPES];

    export let selected: RequestBody;

    let popoverOpen = false;

    async function handleSelect(value: string, triggerId: string) {
        selected = value as RequestBody;
        popoverOpen = false;

        await tick();
        const triggerElement = document.getElementById(triggerId);
        if (triggerElement) {
            triggerElement.focus();
        }
    }
</script>

<Popover bind:open={popoverOpen} let:ids>
    <div class="flex h-9 items-center justify-start gap-3 border-b px-3">
        <span>Content Type</span>
        <PopoverTrigger asChild let:builder>
            <Button
                builders={[builder]}
                variant="outline"
                role="combobox"
                aria-expanded={popoverOpen}
                class="w-fit justify-between"
            >
                <span>{selected}</span>
                <ChevronDown class="ml-2 h-4 w-4 shrink-0 opacity-50" />
            </Button>
        </PopoverTrigger>
    </div>
    <PopoverContent class="w-72 p-0" align="start">
        <Command>
            <CommandGroup>
                {#each Object.values(REQUEST_BODY_TYPES) as BODY_TYPE}
                    <CommandItem
                        value={BODY_TYPE}
                        onSelect={value => handleSelect(value, ids.trigger)}
                        class="flex h-6 justify-between"
                    >
                        <span>{BODY_TYPE}</span>
                        <DotFilled
                            class={cn("h-4 w-4", selected !== BODY_TYPE && "text-transparent")}
                        />
                    </CommandItem>
                {/each}
            </CommandGroup>
        </Command>
    </PopoverContent>
</Popover>
