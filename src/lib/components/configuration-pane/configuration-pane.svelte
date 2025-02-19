<script lang="ts">
    import type { PaneAPI } from "paneforge";
    import { ChevronDownIcon, ChevronUpIcon } from "lucide-svelte";

    import { KeyValueList } from "$lib/components/key-value-list";
    import { Tabs, TabsList, TabsTrigger, TabsContent } from "$lib/components/primitives/tabs";
    import { REQUEST_BODY_TYPES } from "$lib/utils/constants";
    import { ConfigurationBody } from "$lib/components/configuration-pane";
    import { Button } from "$lib/components/primitives/button";
    import { cn } from "$lib/utils/style";

    type Props = {
        pane: PaneAPI;
        isCollapsed: boolean;
        parameters: [boolean, string, string][];
        headers: [boolean, string, string][];
    };

    let {
        pane,
        isCollapsed = $bindable(),
        parameters = $bindable(),
        headers = $bindable(),
    }: Props = $props();

    let body = $state(REQUEST_BODY_TYPES.None);
</script>

<div class="size-full bg-card">
    <Tabs value="parameters">
        <div
            class={cn(
                "flex h-8 w-full items-center justify-between border-y bg-accent/25",
                isCollapsed ? "border-b-transparent" : "",
            )}
        >
            {#if isCollapsed}
                <div class="flex size-full items-center justify-end">
                    <Button
                        variant="ghost"
                        onclick={() => {
                            pane.expand();
                            pane.resize(40);
                        }}
                        class="hover:bg-transparent"
                    >
                        <span class="pr-1.5 text-xs font-medium">Configuration</span>
                        <ChevronDownIcon size={14} />
                    </Button>
                </div>
            {:else}
                <div class="px-1.5">
                    <TabsList
                        class="grid auto-cols-min grid-flow-col justify-start gap-2 p-0 [&>*]:text-xs"
                    >
                        <TabsTrigger value="parameters">Parameters</TabsTrigger>
                        <TabsTrigger value="headers">Headers</TabsTrigger>
                        <TabsTrigger value="body">Body</TabsTrigger>
                    </TabsList>
                </div>
                <div class="flex size-full items-center justify-end">
                    <Button
                        variant="ghost"
                        onclick={() => {
                            pane.collapse();
                        }}
                        class="hover:bg-transparent"
                    >
                        <span class="pr-1.5 text-xs font-medium">Configuration</span>
                        <ChevronUpIcon size={14} />
                    </Button>
                </div>
            {/if}
        </div>

        <div>
            <TabsContent value="parameters" class="m-0 p-3">
                <div class="mb-3">Query Parameters</div>
                <KeyValueList type="parameter" bind:pairs={parameters} />
            </TabsContent>
            <TabsContent value="headers" class="m-0 p-3">
                <div class="mb-3">Headers</div>
                <KeyValueList type="header" bind:pairs={headers} />
            </TabsContent>
            <TabsContent value="body" class="m-0">
                <ConfigurationBody bind:selected={body} />
            </TabsContent>
        </div>
    </Tabs>
</div>
